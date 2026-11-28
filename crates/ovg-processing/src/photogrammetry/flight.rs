//! Trusted host experimental flight orchestration, not a Desktop IPC command.
//! Separate reconstruction and georeferencing gates. No synthetic flat DEM fallback.
use super::{autotune,hardware,ingest,images,colmap,local_worker,colmap_model,jpeg_source,ortho,pipeline,disk_store};
use sha2::{Digest,Sha256};
use std::{fs::{self,File},io::Read,path::{Path,PathBuf},collections::{BTreeMap,BTreeSet},time::{Duration,Instant},sync::atomic::{AtomicBool,Ordering}};
#[derive(Debug)]pub enum Error{Invalid,Limit,Cancelled,Unsupported,AmbiguousModel,Registration,Io(std::io::Error),Tune(autotune::Error),Ingest(ingest::Error),Worker(local_worker::Error),Render(pipeline::Error)}
impl From<std::io::Error> for Error{fn from(e:std::io::Error)->Self{Self::Io(e)}}
#[derive(Debug,Clone,Copy,PartialEq,Eq)]pub enum Stage{Import,Sparse,Undistort,AwaitAlignment,Render,Complete}
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
