//! COLMAP 4.x command planning. This module never launches a process.
//! The qualified host must verify the pack digest, stage scoped inputs, enforce
//! process-tree limits, and validate outputs before publishing an artifact.
use super::Device;
use std::ffi::OsString;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pairing { ExhaustiveSmall, Sequential { overlap: u16 } }
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stage { Extract, Match, Map, Undistort, Depth, Fuse, ConvertModel }
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlanError { InvalidRoot, InvalidSettings, UnsupportedDevice, TooManyImages, MissingModel }
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Step { pub stage: Stage, pub args: Vec<OsString> }
#[derive(Debug, Clone)]
pub struct Config {
    pub image_count: u32, pub max_image_side: u32, pub threads: u16,
    pub device: Device, pub gpu_index: Option<u16>, pub pairing: Pairing,
}
#[derive(Debug, Clone)]
pub struct Layout { root: PathBuf }
impl Layout {
    /// Only a host-created, canonical, isolated staging directory is allowed.
    /// Canonicalization, symlink checks and ownership checks are host obligations.
    pub fn from_host_root(root: PathBuf) -> Result<Self, PlanError> {
        if !root.is_absolute() || root.components().any(|c| matches!(c, std::path::Component::ParentDir | std::path::Component::CurDir)) {
            return Err(PlanError::InvalidRoot);
        }
        Ok(Self { root })
    }
    pub fn images(&self) -> PathBuf { self.root.join("images") }
    pub fn database(&self) -> PathBuf { self.root.join("database.db") }
    pub fn sparse(&self) -> PathBuf { self.root.join("sparse") }
    pub fn dense(&self) -> PathBuf { self.root.join("dense") }
}
fn arg(args: &mut Vec<OsString>, key: &str, value: impl AsRef<std::ffi::OsStr>) {
    args.push(key.into()); args.push(value.as_ref().to_owned());
}
fn step(stage: Stage, cmd: &str) -> Step { Step { stage, args: vec![cmd.into()] } }
fn path_arg(args: &mut Vec<OsString>, key: &str, p: &Path) { arg(args,key,p.as_os_str()); }
fn validate(c: &Config) -> Result<(), PlanError> {
    if c.image_count < 2 || c.max_image_side < 128 || c.max_image_side > 16384 || c.threads == 0 || c.threads > 256 {
        return Err(PlanError::InvalidSettings);
    }
    match (c.device,c.gpu_index) {
        (Device::Cpu,None) | (Device::Cuda,Some(_)) | (Device::Hip,Some(_)) => {},
        _ => return Err(PlanError::UnsupportedDevice),
    }
    match c.pairing {
        Pairing::ExhaustiveSmall if c.image_count > 250 => Err(PlanError::TooManyImages),
        Pairing::Sequential { overlap } if overlap == 0 || overlap > 200 => Err(PlanError::InvalidSettings),
        _ => Ok(()),
    }
}
fn compute(args: &mut Vec<OsString>, prefix: &str, c: &Config) {
    arg(args,&format!("--{prefix}.use_gpu"),if c.device == Device::Cpu {"0"} else {"1"});
    arg(args,&format!("--{prefix}.num_threads"),c.threads.to_string());
    if let Some(i)=c.gpu_index { arg(args,&format!("--{prefix}.gpu_index"),i.to_string()); }
}
/// This is a technical-spike plan, not a production partitioning algorithm.
/// Sequential pairing assumes host-provided ordered flight images.
pub fn sparse_plan(layout: &Layout, c: &Config) -> Result<Vec<Step>,PlanError> {
    validate(c)?;
    let mut extract=step(Stage::Extract,"feature_extractor");
    path_arg(&mut extract.args,"--database_path",&layout.database());
    path_arg(&mut extract.args,"--image_path",&layout.images());
    compute(&mut extract.args,"FeatureExtraction",c);
    arg(&mut extract.args,"--FeatureExtraction.max_image_size",c.max_image_side.to_string());
    let mut matching=step(Stage::Match,match c.pairing {
        Pairing::ExhaustiveSmall => "exhaustive_matcher",
        Pairing::Sequential { .. } => "sequential_matcher",
    });
    path_arg(&mut matching.args,"--database_path",&layout.database());
    compute(&mut matching.args,"FeatureMatching",c);
    if let Pairing::Sequential { overlap }=c.pairing {
        arg(&mut matching.args,"--SequentialMatching.overlap",overlap.to_string());
    }
    let mut map=step(Stage::Map,"mapper");
    path_arg(&mut map.args,"--database_path",&layout.database());
    path_arg(&mut map.args,"--image_path",&layout.images());
    path_arg(&mut map.args,"--output_path",&layout.sparse());
    arg(&mut map.args,"--Mapper.num_threads",c.threads.to_string());
    Ok(vec![extract,matching,map])
}
/// CPU-compatible preparation of pinhole images for the reference ortho pipeline.
/// COLMAP's dense workspace contains images/ and sparse/ (binary model).
/// The host must create a fresh model-text directory and verify actual outputs.
pub fn ortho_prepare_plan(layout:&Layout,c:&Config,model_index:u32)->Result<Vec<Step>,PlanError>{
    validate(c)?;
    let mut undistort=step(Stage::Undistort,"image_undistorter");
    path_arg(&mut undistort.args,"--image_path",&layout.images());
    path_arg(&mut undistort.args,"--input_path",&layout.sparse().join(model_index.to_string()));
    path_arg(&mut undistort.args,"--output_path",&layout.dense());
    arg(&mut undistort.args,"--output_type","COLMAP");
    arg(&mut undistort.args,"--max_image_size",c.max_image_side.to_string());
    let mut convert=step(Stage::ConvertModel,"model_converter");
    path_arg(&mut convert.args,"--input_path",&layout.dense().join("sparse"));
    path_arg(&mut convert.args,"--output_path",&layout.root.join("model-text"));
    arg(&mut convert.args,"--output_type","TXT");
    Ok(vec![undistort,convert])
}
/// Dense output is a scratch PLY, not a georeferenced point cloud.
/// Host selects and validates a real registered sparse model before planning.
pub fn dense_plan(layout: &Layout,c: &Config, model_index: Option<u32>, source_views:u16, cache_gib:u16) -> Result<Vec<Step>,PlanError> {
    validate(c)?;
    if !matches!(c.device,Device::Cuda|Device::Hip) { return Err(PlanError::UnsupportedDevice); }
    let i=model_index.ok_or(PlanError::MissingModel)?;
    if source_views==0 || source_views>64 || cache_gib==0 || cache_gib>128 { return Err(PlanError::InvalidSettings); }
    let mut undistort=step(Stage::Undistort,"image_undistorter");
    path_arg(&mut undistort.args,"--image_path",&layout.images());
    path_arg(&mut undistort.args,"--input_path",&layout.sparse().join(i.to_string()));
    path_arg(&mut undistort.args,"--output_path",&layout.dense());
    arg(&mut undistort.args,"--output_type","COLMAP");
    arg(&mut undistort.args,"--max_image_size",c.max_image_side.to_string());
    let mut depth=step(Stage::Depth,"patch_match_stereo");
    path_arg(&mut depth.args,"--workspace_path",&layout.dense());
    arg(&mut depth.args,"--workspace_format","COLMAP");
    arg(&mut depth.args,"--PatchMatchStereo.gpu_index",c.gpu_index.ok_or(PlanError::UnsupportedDevice)?.to_string());
    arg(&mut depth.args,"--PatchMatchStereo.max_image_size",c.max_image_side.to_string());
    arg(&mut depth.args,"--PatchMatchStereo.cache_size",cache_gib.to_string());
    // Host must rewrite and validate patch-match.cfg after undistort:
    // bound each reference to source_views qualified source images.
    let mut fuse=step(Stage::Fuse,"stereo_fusion");
    path_arg(&mut fuse.args,"--workspace_path",&layout.dense());
    arg(&mut fuse.args,"--workspace_format","COLMAP");
    arg(&mut fuse.args,"--input_type","geometric");
    arg(&mut fuse.args,"--StereoFusion.cache_size",cache_gib.to_string());
    arg(&mut fuse.args,"--StereoFusion.num_threads",c.threads.to_string());
    path_arg(&mut fuse.args,"--output_path",&layout.dense().join("fused.ply"));
    Ok(vec![undistort,depth,fuse])
}

/// A bounded patch-match.cfg generated from registered, host-verified image names.
/// Reject duplicate references and ambiguous delimiter characters; do not allow
/// COLMAP's unbounded __all__ mode or arbitrary source paths.
pub fn patch_match_config(names: &[String], max_source_views: u16) -> Result<String, PlanError> {
    use std::collections::BTreeSet;
    if names.len() < 2 || names.len() > 100_000 || max_source_views == 0 || max_source_views > 64 {
        return Err(PlanError::InvalidSettings);
    }
    let mut unique = BTreeSet::new();
    let mut result = String::new();
    let views = usize::from(max_source_views).min(names.len() - 1);
    for name in names {
        if name.is_empty() || name.len() > 255 || name.starts_with('.') ||
            !name.bytes().all(|b| b.is_ascii_alphanumeric() || matches!(b,b'.'|b'_'|b'-')) ||
            name.starts_with("__") || !unique.insert(name) {
            return Err(PlanError::InvalidSettings);
        }
        result.push_str(name);
        result.push('\n');
        result.push_str(&format!("__auto__, {views}\n"));
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn layout()->Layout { Layout::from_host_root(std::env::temp_dir().join("job with spaces")).unwrap() }
    fn config()->Config { Config { image_count:20,max_image_side:1600,threads:2,device:Device::Cpu,gpu_index:None,pairing:Pairing::ExhaustiveSmall } }
    #[test] fn cpu_sparse_uses_bounded_args() {
        let p=sparse_plan(&layout(),&config()).unwrap();
        assert_eq!(p.iter().map(|s|s.stage).collect::<Vec<_>>(),vec![Stage::Extract,Stage::Match,Stage::Map]);
        assert!(p[0].args.windows(2).any(|w|w[0]=="--FeatureExtraction.use_gpu" && w[1]=="0"));
        assert!(p[0].args.iter().any(|a|a==layout().images().as_os_str()));
    }
    #[test] fn refuses_exhaustive_large_job() {
        let mut c=config();c.image_count=251;
        assert_eq!(sparse_plan(&layout(),&c),Err(PlanError::TooManyImages));
    }
    #[test] fn refuses_unqualified_device_and_model() {
        let mut c=config();
        assert_eq!(dense_plan(&layout(),&c,Some(0),8,1),Err(PlanError::UnsupportedDevice));
        c.device=Device::Cuda;c.gpu_index=Some(0);
        assert_eq!(dense_plan(&layout(),&c,None,8,1),Err(PlanError::MissingModel));
        assert_eq!(dense_plan(&layout(),&c,Some(0),8,1).unwrap().len(),3);
    }
    #[test] fn ortho_preparation_needs_no_dense_gpu(){
        let p=ortho_prepare_plan(&layout(),&config(),0).unwrap();
        assert_eq!(p.iter().map(|s|s.stage).collect::<Vec<_>>(),vec![Stage::Undistort,Stage::ConvertModel]);
        assert!(p[1].args.iter().any(|s|s=="TXT"));
        assert!(p[0].args.iter().any(|s|s==layout().sparse().join("0").as_os_str()));
    }
    #[test] fn bounded_source_config() {
        let names=vec!["a.jpg".into(),"b.jpg".into()];
        assert_eq!(patch_match_config(&names,8).unwrap(),"a.jpg\n__auto__, 1\nb.jpg\n__auto__, 1\n");
        assert!(patch_match_config(&["../a.jpg".into(),"b.jpg".into()],8).is_err());
        assert!(patch_match_config(&["a.jpg".into(),"a.jpg".into()],8).is_err());
    }
    #[test] fn refuses_relative_root_and_invalid_settings() {
        assert!(Layout::from_host_root("relative".into()).is_err());
        let mut c=config();c.threads=0;
        assert_eq!(sparse_plan(&layout(),&c),Err(PlanError::InvalidSettings));
    }
}
