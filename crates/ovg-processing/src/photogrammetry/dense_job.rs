//! Dense reconstruction orchestration with an explicit host GPU containment boundary.
//! No direct device mount: only a qualified executor can run these commands.
use super::{colmap,Device,Budget};
use std::path::Path;
#[derive(Debug,Clone,Copy,PartialEq,Eq)]pub enum Error{Invalid,Unqualified,Budget,Plan,Cancelled,Worker,Output}
pub struct Qualification{pub build_sha256:String,pub device:Device,pub physical_gpu:String,pub driver:String,pub max_budget:Budget,pub aggregate_limits:bool,pub offline:bool}
pub struct Job<'a>{pub config:&'a colmap::Config,pub model_index:u32,pub max_source_views:u16,pub cache_gib:u16,pub requested:Budget,pub expected_build:&'a str}
/// Trusted host implementation must verify pack, bind only the selected GPU,
/// reserve resources, enforce aggregate quotas and kill the complete process tree.
pub trait Executor{
 fn qualification(&self)->&Qualification;
 fn execute(&mut self,step:&colmap::Step,cancelled:&mut dyn FnMut()->bool)->Result<(),Error>;
 fn registered_images(&mut self)->Result<Vec<String>,Error>;
 fn install_patch_match_config(&mut self,config:&str)->Result<(),Error>;
 fn verify_fused_cloud(&mut self)->Result<u64,Error>;
}
pub fn run(executor:&mut impl Executor,root:&Path,job:Job<'_>,mut cancelled:impl FnMut()->bool)->Result<u64,Error>{
 let q=executor.qualification();
 if !matches!(job.config.device,Device::Cuda|Device::Hip)||q.device!=job.config.device||
 !q.aggregate_limits||!q.offline||!super::valid_id(&q.physical_gpu)||q.driver.is_empty()||
 !super::digest(job.expected_build)||q.build_sha256!=job.expected_build{return Err(Error::Unqualified);}
 if job.requested.device_bytes==0||!job.requested.fits(q.max_budget)||u16::from(job.config.threads)>job.requested.threads||
 u64::from(job.cache_gib)*(1<<30)>job.requested.host_bytes/2{return Err(Error::Budget);}
 let layout=colmap::Layout::from_host_root(root.to_path_buf()).map_err(|_|Error::Plan)?;
 let plan=colmap::dense_plan(&layout,job.config,Some(job.model_index),job.max_source_views,job.cache_gib).map_err(|_|Error::Plan)?;
 if cancelled(){return Err(Error::Cancelled);}
 executor.execute(&plan[0],&mut cancelled)?;
 if cancelled(){return Err(Error::Cancelled);}
 let names=executor.registered_images()?;
 if names.len()>job.config.image_count as usize{return Err(Error::Output);}
 let cfg=colmap::patch_match_config(&names,job.max_source_views).map_err(|_|Error::Output)?;
 executor.install_patch_match_config(&cfg)?;
 for step in &plan[1..]{if cancelled(){return Err(Error::Cancelled);}executor.execute(step,&mut cancelled)?;}
 if cancelled(){return Err(Error::Cancelled);}
 let count=executor.verify_fused_cloud()?;if count==0{return Err(Error::Output);}Ok(count)
}
#[cfg(test)]mod tests{
 use super::*;
 struct Mock{q:Qualification,stages:Vec<colmap::Stage>,configured:bool}
 impl Executor for Mock{
 fn qualification(&self)->&Qualification{&self.q}
 fn execute(&mut self,s:&colmap::Step,_:&mut dyn FnMut()->bool)->Result<(),Error>{
 if s.stage==colmap::Stage::Depth&&!self.configured{return Err(Error::Worker);}self.stages.push(s.stage);Ok(())}
 fn registered_images(&mut self)->Result<Vec<String>,Error>{Ok(vec!["a.jpg".into(),"b.jpg".into()])}
 fn install_patch_match_config(&mut self,c:&str)->Result<(),Error>{self.configured=c.contains("__auto__, 1");Ok(())}
 fn verify_fused_cloud(&mut self)->Result<u64,Error>{Ok(100)}
 }
 fn budget()->Budget{Budget{host_bytes:8<<30,device_bytes:4<<30,scratch_bytes:20<<30,threads:4}}
 fn config()->colmap::Config{colmap::Config{image_count:2,max_image_side:1600,threads:4,device:Device::Cuda,gpu_index:Some(0),pairing:colmap::Pairing::ExhaustiveSmall}}
 fn mock()->Mock{Mock{q:Qualification{build_sha256:"a".repeat(64),device:Device::Cuda,physical_gpu:"gpu0".into(),driver:"qualified".into(),max_budget:budget(),aggregate_limits:true,offline:true},stages:vec![],configured:false}}
 #[test]fn orders_stages_and_bounds_source_views(){
 let mut m=mock();let c=config();let hash="a".repeat(64);
 let j=Job{config:&c,model_index:0,max_source_views:8,cache_gib:1,requested:budget(),expected_build:&hash};
 assert_eq!(run(&mut m,&std::env::temp_dir(),j,||false),Ok(100));
 assert_eq!(m.stages,vec![colmap::Stage::Undistort,colmap::Stage::Depth,colmap::Stage::Fuse]);}
 #[test]fn refuses_uncontained_gpu(){
 let mut m=mock();m.q.aggregate_limits=false;let c=config();let hash="a".repeat(64);
 let j=Job{config:&c,model_index:0,max_source_views:8,cache_gib:1,requested:budget(),expected_build:&hash};
 assert_eq!(run(&mut m,&std::env::temp_dir(),j,||false),Err(Error::Unqualified));assert!(m.stages.is_empty());}
}
