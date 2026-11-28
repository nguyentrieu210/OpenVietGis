//! Trusted host experimental flight orchestration, not a Desktop IPC command.
//! Separate reconstruction and georeferencing gates. No synthetic flat DEM fallback.
use super::{autotune,hardware,ingest,images,colmap,local_worker,colmap_model,jpeg_source,ortho,pipeline,disk_store};
use sha2::{Digest,Sha256};
use std::{fs::{self,File},io::Read,path::{Path,PathBuf},collections::{BTreeMap,BTreeSet},time::{Duration,Instant},sync::atomic::{AtomicBool,Ordering}};
#[derive(Debug)]pub enum Error{Invalid,Limit,Cancelled,Unsupported,AmbiguousModel,Registration,Io(std::io::Error),Tune(autotune::Error),Ingest(ingest::Error),Worker(local_worker::Error),Render(pipeline::Error),Control(super::control_network::Error),Checkpoint,Surface(super::cloud_dsm::Error),Dense(super::cuda_candidate::Error)}
impl From<std::io::Error> for Error{fn from(e:std::io::Error)->Self{Self::Io(e)}}
#[derive(Debug,Clone,Copy,PartialEq,Eq)]pub enum Stage{Import,Sparse,Undistort,AwaitAlignment,Dense,Surface,Render,Complete}
pub struct PreparedFlight{root:PathBuf,model:local_worker::Prepared,plan:autotune::Plan,pub input_sha256:String,pub model_sha256:String,pub registered:usize,pub total:usize}
fn digest(path:&Path,cap:u64,cancel:&AtomicBool)->Result<String,Error>{
 let m=fs::symlink_metadata(path)?;if !m.file_type().is_file()||m.len()==0||m.len()>cap{return Err(Error::Limit);}
 let mut f=File::open(path)?;let mut h=Sha256::new();let mut b=[0;65536];let mut n=0u64;
 loop{if cancel.load(Ordering::Acquire){return Err(Error::Cancelled);}let k=f.read(&mut b)?;if k==0{break;}n+=k as u64;if n>m.len(){return Err(Error::Invalid);}h.update(&b[..k]);}
 if n!=m.len(){return Err(Error::Invalid);}Ok(format!("{:x}",h.finalize()))
}
fn model_index(root:&Path)->Result<u32,Error>{
 let mut models=Vec::new();
 for e in fs::read_dir(root.join("sparse"))?{
 let e=e?;if !e.file_type()?.is_dir(){continue;}
 let name=e.file_name();let n=name.to_str().ok_or(Error::Invalid)?;
 let i=n.parse::<u32>().map_err(|_|Error::Invalid)?;if i.to_string()!=n{return Err(Error::Invalid);}
 if !["cameras.bin","images.bin","points3D.bin"].iter().all(|n|fs::symlink_metadata(e.path().join(n)).is_ok_and(|m|m.file_type().is_file()&&m.len()>0)){return Err(Error::Invalid);}
 models.push(i);if models.len()>1{return Err(Error::AmbiguousModel);}
 }
 models.pop().ok_or(Error::Registration)
}
fn model_digest(root:&Path,views:&[colmap_model::View],cancel:&AtomicBool)->Result<String,Error>{
 let mut h=Sha256::new();h.update(b"ovg-prepared-model-v1\0");
 for n in ["cameras.txt","images.txt","points3D.txt"]{
 h.update(n.as_bytes());h.update(digest(&root.join("model-text").join(n),256<<20,cancel)?.as_bytes());
 }
 for n in ["cameras.bin","images.bin","points3D.bin"]{
 h.update(n.as_bytes());h.update(digest(&root.join("dense/sparse").join(n),512<<20,cancel)?.as_bytes());
 }
 let mut total=0u64;let mut names=BTreeSet::new();
 for v in views{
 if !colmap_model::safe_name(&v.name)||!names.insert(&v.name){return Err(Error::Invalid);}
 let p=root.join("dense/images").join(&v.name);
 total=total.checked_add(fs::symlink_metadata(&p)?.len()).ok_or(Error::Limit)?;
 if total>8*hardware::GIB{return Err(Error::Limit);}
 h.update((v.name.len()as u32).to_le_bytes());h.update(v.name.as_bytes());h.update(digest(&p,128<<20,cancel)?.as_bytes());
 }
 Ok(format!("{:x}",h.finalize()))
}
/// Host must provide private fresh root, approved immutable COLMAP pack, real
/// filesystem quota and qualified Linux sandbox. No network/GPU fallback.
pub fn prepare(root:&Path,paths:&[PathBuf],pack:&local_worker::ApprovedPack,telemetry:hardware::Snapshot,
 output_pixels:u64,ordered_flight:bool,timeout:Duration,cancel:&AtomicBool,mut progress:impl FnMut(Stage,u64,u64))->Result<PreparedFlight,Error>{
 if !cfg!(target_os="linux"){return Err(Error::Unsupported);}
 if !root.is_absolute()||root.exists()||timeout.is_zero()||timeout>Duration::from_secs(86400)||paths.len()<2||paths.len()>4096{return Err(Error::Invalid);}
 let stop=||cancel.load(Ordering::Acquire);
 // Inspect before admitting any large copy.
 let header=images::Limits{max_header_bytes:1024*1024,max_side:32768,max_pixels:80_000_000};
 let(mut largest,mut bytes)=(images::Dimensions{width:0,height:0},0u64);
 for path in paths{
 if stop(){return Err(Error::Cancelled);}let m=fs::symlink_metadata(path)?;
 if !m.file_type().is_file()||m.len()==0||m.len()>128<<20{return Err(Error::Invalid);}
 bytes=bytes.checked_add(m.len()).ok_or(Error::Limit)?;
 let mut b=Vec::new();File::open(path)?.take(header.max_header_bytes as u64).read_to_end(&mut b)?;
 let d=images::jpeg_dimensions(&b,header).map_err(|_|Error::Invalid)?;
 largest.width=largest.width.max(d.width);largest.height=largest.height.max(d.height);
 }
 let plan=autotune::plan(telemetry,autotune::Workload{images:paths.len()as u32,largest,input_bytes:bytes,output_pixels,ordered_flight}).map_err(Error::Tune)?;
 let started=Instant::now();
 progress(Stage::Import,0,bytes);
 let entries=ingest::stage_jpeg_controlled(root,paths,ingest::Policy{max_files:4096,max_file_bytes:128<<20,max_total_bytes:bytes,header},stop,|_,b|progress(Stage::Import,b,bytes)).map_err(Error::Ingest)?;
 if entries.iter().any(|e|e.dimensions.width>largest.width||e.dimensions.height>largest.height){return Err(Error::Invalid);}
 let input=ingest::verify_staged(root,paths.len()as u32,bytes,stop).map_err(Error::Ingest)?;
 let limits=||->Result<local_worker::Limits,Error>{
 let remaining=timeout.checked_sub(started.elapsed()).filter(|d|!d.is_zero()).ok_or(Error::Limit)?;
 Ok(local_worker::Limits{timeout:remaining,address_space_bytes:plan.admission.budget.host_bytes,max_file_bytes:plan.admission.budget.scratch_bytes.min(16*hardware::GIB),input_bytes:bytes})
 };
 progress(Stage::Sparse,0,1);
 local_worker::sparse(root,pack,&plan.sparse,limits()?,&input,stop).map_err(Error::Worker)?;
 let index=model_index(root)?;
 progress(Stage::Undistort,0,1);
 let model=local_worker::prepare_ortho(root,pack,&plan.sparse,limits()?,&input,index,stop).map_err(Error::Worker)?;
 let registered=model.views.len();
 if registered*100<paths.len()*95{return Err(Error::Registration);}
 let model_sha256=model_digest(root,&model.views,cancel)?;
 if stop(){return Err(Error::Cancelled);}
 progress(Stage::AwaitAlignment,registered as u64,paths.len()as u64);
 Ok(PreparedFlight{root:root.to_path_buf(),model,plan,input_sha256:input,model_sha256,registered,total:paths.len()})
}
/// Alignment and external DEM must be independently qualified in the same metric
/// CRS and bound to this exact reconstructed model by the trusted host.
pub struct AlignedSurface{pub model_sha256:String,pub alignment:colmap_model::Alignment,pub dem:PathBuf,pub dem_sha256:String,pub grid:ortho::Grid,pub epsg:u16,pub footprints:BTreeMap<String,super::tiles::Window>}
pub fn render(flight:&PreparedFlight,s:&AlignedSurface,output:&Path,cancel:&AtomicBool,mut progress:impl FnMut(Stage,u64,u64))->Result<pipeline::ResultSummary,Error>{
 if !output.is_absolute()||output.exists()||s.model_sha256!=flight.model_sha256||
 !((32601..=32660).contains(&s.epsg)||(32701..=32760).contains(&s.epsg))||!super::digest(&s.dem_sha256){return Err(Error::Invalid);}
 let n=u64::from(s.grid.width)*u64::from(s.grid.height);
 if n==0||n>64_000_000||n.checked_mul(4).ok_or(Error::Limit)?>flight.plan.render.output_bytes{return Err(Error::Limit);}
 if !s.dem.is_absolute()||fs::symlink_metadata(&s.dem)?.len()!=n*4||digest(&s.dem,n*4,cancel)?!=s.dem_sha256||
 model_digest(&flight.root,&flight.model.views,cancel)?!=flight.model_sha256{return Err(Error::Invalid);}
 // Current adapter decodes full JPEGs and trusts conservative footprints.
 // Until the host has verified those footprints, it should pass an empty map.
 let mut src=jpeg_source::Source::from_model(&flight.model.images,s.dem.clone(),s.grid.width,s.grid.height,
 &flight.model.views,s.alignment,&s.footprints,jpeg_source::Policy{max_compressed_bytes:128<<20,
 max_decode_bytes:flight.plan.decode_bytes,cache_bytes:flight.plan.cache_bytes,
 header:images::Limits{max_header_bytes:1024*1024,max_side:16384,max_pixels:80_000_000}}).map_err(Error::Render)?;
 fs::create_dir(output)?;let store=disk_store::Store::open(output).map_err(|e|Error::Render(pipeline::Error::Store(e)))?;
 progress(Stage::Render,0,0);
 let q=pipeline::run_controlled(&mut src,&store,0,"preview",s.grid,s.epsg,flight.plan.render,||cancel.load(Ordering::Acquire),|n,t|progress(Stage::Render,n as u64,t as u64)).map_err(Error::Render)?;
 progress(Stage::Complete,q.tiles as u64,q.tiles as u64);Ok(q)
}

/// GCP pixels refer to the *undistorted* prepared images, not original JPEGs.
/// Target coordinates and vertical datum are supplied by the survey host.
#[derive(Debug,Clone,Copy)]pub struct ImageMark{pub view_id:u32,pub pixel:[f64;2],pub sigma_px:f64}
pub struct GroundMark<'a>{pub id:&'a str,pub role:super::control_network::Role,pub target:[f64;3],pub sigma_m:f64,pub images:&'a[ImageMark]}
pub struct QualifiedAlignment{
 model_sha256:String,alignment:colmap_model::Alignment,epsg:u16,vertical_datum:String,
 pub report:super::control_network::Report,
}
pub fn qualify_alignment(flight:&PreparedFlight,expected_model:&str,epsg:u16,vertical_datum:&str,
 marks:&[GroundMark<'_>],policy:super::control_network::Policy,cancel:&AtomicBool)->Result<QualifiedAlignment,Error>{
 use super::{control_network,gcp};
 if expected_model!=flight.model_sha256||!((32601..=32660).contains(&epsg)||(32701..=32760).contains(&epsg))||
 vertical_datum.trim().is_empty()||vertical_datum.len()>128||vertical_datum.chars().any(char::is_control)||
 marks.len()>512||policy.min_controls<3||policy.min_checks<3{return Err(Error::Invalid);}
 if model_digest(&flight.root,&flight.model.views,cancel)?!=flight.model_sha256{return Err(Error::Invalid);}
 let views:BTreeMap<_,_>=flight.model.views.iter().map(|v|(v.id,v)).collect();
 if views.len()!=flight.model.views.len(){return Err(Error::Invalid);}
 let mut observations=Vec::with_capacity(marks.len());
 let mut total=0usize;
 for m in marks{
 if cancel.load(Ordering::Acquire){return Err(Error::Cancelled);}
 if m.images.len()<2||m.images.len()>64{return Err(Error::Invalid);}
 total=total.checked_add(m.images.len()).ok_or(Error::Limit)?;if total>16384{return Err(Error::Limit);}
 super::preview::lonlat(epsg,m.target[0],m.target[1]).map_err(|_|Error::Invalid)?;
 let mut seen=BTreeSet::new();let mut obs=Vec::new();
 for o in m.images{
 let v=views.get(&o.view_id).ok_or(Error::Invalid)?;
 if !seen.insert(o.view_id)||!o.pixel.iter().all(|v|v.is_finite())||
 o.pixel[0]<0.||o.pixel[1]<0.||o.pixel[0]>=f64::from(v.width)||o.pixel[1]>=f64::from(v.height)||
 !o.sigma_px.is_finite()||o.sigma_px<=0.{return Err(Error::Invalid);}
 obs.push(gcp::Observation{camera_id:o.view_id,camera:v.camera,pixel:o.pixel,sigma_px:o.sigma_px});
 }
 observations.push(obs);
 }
 let input:Vec<_>=marks.iter().zip(&observations).map(|(m,o)|control_network::Mark{
 id:m.id,role:m.role,target:m.target,sigma_m:m.sigma_m,observations:o}).collect();
 let report=control_network::align(&input,policy).map_err(Error::Control)?;
 if cancel.load(Ordering::Acquire){return Err(Error::Cancelled);}
 if !report.passed{return Err(Error::Checkpoint);}
 Ok(QualifiedAlignment{model_sha256:flight.model_sha256.clone(),alignment:report.alignment,epsg,vertical_datum:vertical_datum.to_owned(),report})
}
/// Host-issued provenance must bind the fused cloud to the same local frame,
/// model hash and vertical datum; a matching digest alone cannot prove geometry.
pub struct CloudInput<'a>{pub path:&'a Path,pub sha256:&'a str,pub model_sha256:&'a str}
pub struct DsmProduct{pub surface:AlignedSurface,pub receipt:super::cloud_dsm::Receipt,pub vertical_datum:String}
pub fn derive_dsm(flight:&PreparedFlight,aligned:&QualifiedAlignment,cloud:CloudInput<'_>,
 grid:ortho::Grid,policy:super::cloud_dsm::Policy,output:&Path,cancel:&AtomicBool)->Result<DsmProduct,Error>{
 if aligned.model_sha256!=flight.model_sha256||cloud.model_sha256!=flight.model_sha256||
 !super::digest(cloud.sha256)||!cloud.path.is_absolute()||!output.is_absolute()||output.exists()||
 policy.max_work_bytes>flight.plan.admission.budget.host_bytes/2{return Err(Error::Invalid);}
 let n=u64::from(grid.width)*u64::from(grid.height);
 if n==0||n>16_000_000||n.checked_mul(4).ok_or(Error::Limit)?>flight.plan.render.output_bytes{return Err(Error::Limit);}
 if digest(cloud.path,policy.cloud.max_bytes,cancel)?!=cloud.sha256||
 model_digest(&flight.root,&flight.model.views,cancel)?!=aligned.model_sha256{return Err(Error::Invalid);}
 // The staging directory is fresh and private. No result becomes visible at
 // the requested path before the input is rechecked.
 let parent=output.parent().ok_or(Error::Invalid)?;
 let stage=parent.join(format!(".flight-dsm-{}",uuid::Uuid::new_v4()));
 fs::create_dir(&stage)?;
 let temp=stage.join("dem.raw");
 let result=(||{
 let receipt=super::cloud_dsm::build(File::open(cloud.path)?,&temp,grid,aligned.alignment,policy,
 ||cancel.load(Ordering::Acquire)).map_err(Error::Surface)?;
 if digest(cloud.path,policy.cloud.max_bytes,cancel)?!=cloud.sha256{return Err(Error::Invalid);}
 if cancel.load(Ordering::Acquire){return Err(Error::Cancelled);}
 fs::hard_link(&temp,output)?;
 Ok(DsmProduct{surface:AlignedSurface{model_sha256:aligned.model_sha256.clone(),alignment:aligned.alignment,
 dem:output.to_path_buf(),dem_sha256:receipt.sha256.clone(),grid,epsg:aligned.epsg,footprints:BTreeMap::new()},
 receipt,vertical_datum:aligned.vertical_datum.clone()})
 })();
 let _=fs::remove_dir_all(&stage);result
}

/// Copy a bounded immutable model artifact to a fresh worker snapshot and
/// compare actual byte digests. No hardlinks into writable provider staging.
fn copy_model(src:&Path,dst:&Path,remaining:u64,cancel:&AtomicBool)->Result<u64,Error>{
 use std::io::Write;
 let m=fs::symlink_metadata(src)?;
 if !m.file_type().is_file()||m.len()==0||m.len()>remaining{return Err(Error::Limit);}
 let mut f=File::open(src)?;let mut o=fs::OpenOptions::new().write(true).create_new(true).open(dst)?;
 let(mut n,mut h)=(0u64,Sha256::new());let mut b=[0;65536];
 loop{if cancel.load(Ordering::Acquire){return Err(Error::Cancelled);}let k=f.read(&mut b)?;if k==0{break;}
 n=n.checked_add(k as u64).ok_or(Error::Limit)?;if n>m.len(){return Err(Error::Invalid);}
 o.write_all(&b[..k])?;h.update(&b[..k]);}
 o.sync_all()?;drop(o);
 if n!=m.len()||format!("{:x}",h.finalize())!=digest(src,remaining,cancel)?{return Err(Error::Invalid);}Ok(n)
}
pub struct DenseProduct{pub dsm:DsmProduct,pub cloud:super::fused_ply::Summary,pub mosaic:pipeline::ResultSummary}
/// Experimental host-only end-to-end GPU route after GCP qualification.
/// All three output paths must be fresh, on host-quota-backed private storage.
/// The caller owns partial-job cleanup. Successful intermediate DSM is retained
/// if the subsequent orthomosaic stage fails; no final mosaic receipt is issued.
pub fn reconstruct_dense(flight:&PreparedFlight,aligned:&QualifiedAlignment,worker:&Path,dem:&Path,output:&Path,
 grid:ortho::Grid,gpu:&super::cuda_candidate::Policy,source_views:u16,cache_gib:u16,
 surface_policy:super::cloud_dsm::Policy,cancel:&AtomicBool,mut progress:impl FnMut(Stage,u64,u64))->Result<DenseProduct,Error>{
 if !cfg!(target_os="linux"){return Err(Error::Unsupported);}
 if aligned.model_sha256!=flight.model_sha256||!worker.is_absolute()||worker.exists()||
 !dem.is_absolute()||dem.exists()||!output.is_absolute()||output.exists()||dem==output||
 worker.starts_with(&flight.root)||dem.starts_with(worker)||output.starts_with(worker)||
 gpu.host_bytes>flight.plan.admission.budget.host_bytes||gpu.threads>flight.plan.admission.budget.threads||
 gpu.max_input_bytes>flight.plan.admission.budget.scratch_bytes||
 gpu.cloud.max_bytes>flight.plan.admission.budget.scratch_bytes||
 surface_policy.max_work_bytes>flight.plan.admission.budget.host_bytes/2{return Err(Error::Invalid);}
 if model_digest(&flight.root,&flight.model.views,cancel)?!=aligned.model_sha256{return Err(Error::Invalid);}
 fs::create_dir(worker)?;
 for n in ["images","sparse","dense","dense/images","dense/sparse"]{fs::create_dir(worker.join(n))?;}
 let mut bytes=0u64;
 let mut names=Vec::with_capacity(flight.registered);
 for v in &flight.model.views{
 let rel=PathBuf::from("dense/images").join(&v.name);
 bytes+=copy_model(&flight.root.join(&rel),&worker.join(&rel),gpu.max_input_bytes-bytes,cancel)?;
 names.push(v.name.clone());
 }
 for n in ["cameras.bin","images.bin","points3D.bin"]{
 let rel=PathBuf::from("dense/sparse").join(n);
 bytes+=copy_model(&flight.root.join(&rel),&worker.join(&rel),gpu.max_input_bytes-bytes,cancel)?;
 }
 if model_digest(&flight.root,&flight.model.views,cancel)?!=aligned.model_sha256{return Err(Error::Invalid);}
 let cfg=colmap::Config{image_count:flight.registered as u32,max_image_side:flight.plan.sparse.max_image_side,
 threads:gpu.threads,device:super::Device::Cuda,gpu_index:Some(0),
 pairing:flight.plan.sparse.pairing.clone()};
 progress(Stage::Dense,0,1);
 let cloud=super::cuda_candidate::run_prepared(worker,&cfg,source_views,cache_gib,gpu,&names,
 ||cancel.load(Ordering::Acquire)).map_err(Error::Dense)?;
 // Recheck the worker input snapshot before trusting its output frame.
 for v in &flight.model.views{
 let rel=PathBuf::from("dense/images").join(&v.name);
 if digest(&worker.join(&rel),gpu.max_input_bytes,cancel)?!=digest(&flight.root.join(&rel),gpu.max_input_bytes,cancel)?{return Err(Error::Invalid);}
 }
 for n in ["cameras.bin","images.bin","points3D.bin"]{
 let rel=PathBuf::from("dense/sparse").join(n);
 if digest(&worker.join(&rel),gpu.max_input_bytes,cancel)?!=digest(&flight.root.join(&rel),gpu.max_input_bytes,cancel)?{return Err(Error::Invalid);}
 }
 let ply=worker.join("dense/fused.ply");let sha=digest(&ply,gpu.cloud.max_bytes,cancel)?;
 progress(Stage::Surface,0,1);
 let dsm=derive_dsm(flight,aligned,CloudInput{path:&ply,sha256:&sha,model_sha256:&aligned.model_sha256},grid,surface_policy,dem,cancel)?;
 let mosaic=render(flight,&dsm.surface,output,cancel,&mut progress)?;
 Ok(DenseProduct{dsm,cloud,mosaic})
}
#[cfg(test)]mod tests{
 use super::*;
 #[test]fn model_selection_fails_on_multiple_components(){
 let p=std::env::temp_dir().join(format!("ovg-flight-{}",uuid::Uuid::new_v4()));
 fs::create_dir_all(p.join("sparse")).unwrap();assert!(matches!(model_index(&p),Err(Error::Registration)));
 for i in 0..2{let d=p.join("sparse").join(i.to_string());fs::create_dir(&d).unwrap();for n in ["cameras.bin","images.bin","points3D.bin"]{fs::write(d.join(n),[1]).unwrap();}
 if i==0{assert_eq!(model_index(&p).unwrap(),0);}}
 assert!(matches!(model_index(&p),Err(Error::AmbiguousModel)));let _=fs::remove_dir_all(p);
 }
}
