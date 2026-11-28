//! Explicitly experimental Linux NVIDIA CDI / Podman dense reconstruction runner.
//! No production qualification: CDI isolation, GPU memory and filesystem quota
//! must be independently verified. No automatic fallback to host execution.
use super::{colmap,fused_ply,Device};
use std::{path::Path,time::Duration};
#[derive(Debug)]pub enum Error{Unsupported,Invalid,Limit,Cancelled,Timeout,Failed,Io(std::io::Error),Cloud(fused_ply::Error)}
impl From<std::io::Error> for Error{fn from(e:std::io::Error)->Self{Self::Io(e)}}
#[derive(Debug,Clone)]
pub struct Policy{
 pub image_id:String,pub gpu_uuid:String,pub host_bytes:u64,pub threads:u16,
 pub timeout:Duration,pub max_input_bytes:u64,pub cloud:fused_ply::Policy,
}
fn valid(p:&Policy)->bool{
 p.image_id.strip_prefix("sha256:").is_some_and(super::digest)&&
 p.gpu_uuid.strip_prefix("GPU-").is_some_and(|s|s.len()==36&&s.bytes().enumerate().all(|(i,b)|
 if [8,13,18,23].contains(&i){b==b'-'}else{b.is_ascii_hexdigit()}))&&
 p.host_bytes>=1024*1024*1024&&p.threads>0&&p.threads<=32&&
 !p.timeout.is_zero()&&p.timeout<=Duration::from_secs(86400)&&p.max_input_bytes>0
}
/// Run only in a dedicated, prequalified rootless test account, on a quota-backed
/// private staging filesystem. Host verifies the local image provenance, CDI
/// device mapping, dependency versions and source immutability. VRAM is NOT capped.
pub fn run(root:&Path,c:&colmap::Config,model_index:u32,source_views:u16,cache_gib:u16,
 approved_manifest:&str,p:&Policy,mut cancelled:impl FnMut()->bool)->Result<fused_ply::Summary,Error>{
 if !valid(p)||!root.is_absolute()||!super::digest(approved_manifest)||c.device!=Device::Cuda||
 c.gpu_index!=Some(0)||c.threads>p.threads||u64::from(cache_gib)*(1<<30)>p.host_bytes/2{return Err(Error::Invalid);}
 #[cfg(not(target_os="linux"))]{let _=(model_index,source_views,&mut cancelled);Err(Error::Unsupported)}
 #[cfg(target_os="linux")]{
 use std::{fs,io::Write,collections::BTreeSet,time::Instant};
 if !Path::new("/usr/bin/podman").is_file(){return Err(Error::Unsupported);}
 let root=root.canonicalize()?;let path=root.to_str().ok_or(Error::Invalid)?;
 // Podman mount option delimiters are not allowed in the staging path.
 if root==Path::new("/")||path.contains([',',':','\n','\r'])||root.join("dense").exists(){return Err(Error::Invalid);}
 for d in [root.join("images"),root.join("sparse"),root.join("sparse").join(model_index.to_string())]{
 if !fs::symlink_metadata(&d)?.file_type().is_dir(){return Err(Error::Invalid);}
 }
 for n in ["cameras.bin","images.bin","points3D.bin"]{
 let m=fs::symlink_metadata(root.join("sparse").join(model_index.to_string()).join(n))?;
 if !m.file_type().is_file()||m.len()==0||m.len()>p.max_input_bytes{return Err(Error::Invalid);}
 }
 let hash=super::ingest::verify_staged(&root,c.image_count,p.max_input_bytes,&mut cancelled).map_err(|_|Error::Invalid)?;
 if hash!=approved_manifest{return Err(Error::Invalid);}
 let mut original=BTreeSet::new();
 for e in fs::read_dir(root.join("images"))?{let e=e?;let n=e.file_name().into_string().map_err(|_|Error::Invalid)?;
 if !e.file_type()?.is_file()||!super::colmap_model::safe_name(&n)||!original.insert(n)||original.len()>c.image_count as usize{return Err(Error::Invalid);}}
 if original.len()!=c.image_count as usize{return Err(Error::Invalid);}
 let layout=colmap::Layout::from_host_root("/work".into()).map_err(|_|Error::Invalid)?;
 let plan=colmap::dense_plan(&layout,c,Some(model_index),source_views,cache_gib).map_err(|_|Error::Invalid)?;
 fs::create_dir(root.join("dense"))?;
 let started=Instant::now();
 step(&root,&plan[0],p,started,&mut cancelled)?;
 let image_dir=root.join("dense/images");
 if !fs::symlink_metadata(&image_dir)?.file_type().is_dir(){return Err(Error::Invalid);}
 let mut names=BTreeSet::new();
 for e in fs::read_dir(&image_dir)?{let e=e?;let n=e.file_name().into_string().map_err(|_|Error::Invalid)?;
 if !e.file_type()?.is_file()||!original.contains(&n)||!names.insert(n){return Err(Error::Invalid);}}
 let cfg=colmap::patch_match_config(&names.into_iter().collect::<Vec<_>>(),source_views).map_err(|_|Error::Invalid)?;
 let stereo=root.join("dense/stereo");
 if !fs::symlink_metadata(&stereo)?.file_type().is_dir(){return Err(Error::Invalid);}
 let target=stereo.join("patch-match.cfg");
 if !fs::symlink_metadata(&target)?.file_type().is_file(){return Err(Error::Invalid);}
 let temp=stereo.join(format!(".cfg-{}",uuid::Uuid::new_v4()));
 let mut f=fs::OpenOptions::new().write(true).create_new(true).open(&temp)?;f.write_all(cfg.as_bytes())?;f.sync_all()?;drop(f);fs::rename(temp,target)?;
 for s in &plan[1..]{step(&root,s,p,started,&mut cancelled)?;}
 let path=root.join("dense/fused.ply");let m=fs::symlink_metadata(&path)?;
 if !m.file_type().is_file()||m.len()>p.cloud.max_bytes{return Err(Error::Invalid);}
 fused_ply::verify(fs::File::open(path)?,p.cloud,&mut cancelled).map_err(Error::Cloud)
 }
}
#[cfg(target_os="linux")]
fn step(root:&Path,s:&colmap::Step,p:&Policy,started:std::time::Instant,cancelled:&mut impl FnMut()->bool)->Result<(),Error>{
 use std::{process::{Command,Stdio},time::Instant};
 if cancelled(){return Err(Error::Cancelled);}
 let remaining=p.timeout.checked_sub(started.elapsed()).ok_or(Error::Timeout)?;
 if remaining.is_zero(){return Err(Error::Timeout);}
 let name=format!("ovg-photo-{}",uuid::Uuid::new_v4());
 let mut cmd=Command::new("/usr/bin/podman");
 // Only fixed flags, no shell, no registry pull, no host network, no privileged mode.
 cmd.args(["run","--rm","--pull=never","--network=none","--cap-drop=all","--security-opt=no-new-privileges",
 "--read-only","--userns=keep-id","--pids-limit=512","--name"]).arg(&name)
 .arg(format!("--memory={}",p.host_bytes)).arg(format!("--memory-swap={}",p.host_bytes))
 .arg(format!("--cpus={}",p.threads)).arg(format!("--timeout={}",remaining.as_secs().max(1)))
 .arg(format!("--device=nvidia.com/gpu={}",p.gpu_uuid))
 .arg("--mount").arg(format!("type=bind,src={},destination=/work",root.display()))
 .arg("--mount").arg(format!("type=bind,src={},destination=/work/images,ro",root.join("images").display()))
 .arg("--mount").arg(format!("type=bind,src={},destination=/work/sparse,ro",root.join("sparse").display()))
 .args(["--workdir=/work","--entrypoint=/usr/bin/colmap","--env=QT_QPA_PLATFORM=offscreen"])
 .arg(&p.image_id).args(&s.args).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null());
 let mut child=cmd.spawn()?;
 let result=loop{
 match child.try_wait(){Ok(Some(v))=>break if v.success(){Ok(())}else{Err(Error::Failed)},Ok(None)=>{},Err(e)=>break Err(Error::Io(e))}
 if cancelled(){break Err(Error::Cancelled);}
 if started.elapsed()>=p.timeout{break Err(Error::Timeout);}
 std::thread::sleep(Duration::from_millis(50));
 };
 if result.is_err(){
 // Best effort explicit container removal; host supervisor must reconcile orphan
 // containers after client crash or cleanup failure. Never claim hard containment.
 let _=child.kill();let _=child.wait();
 if let Ok(mut cleanup)=Command::new("/usr/bin/podman").args(["rm","-f",&name]).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null()).spawn(){
 let t=Instant::now();loop{if cleanup.try_wait().is_ok_and(|v|v.is_some()){break;}
 if t.elapsed()>=Duration::from_secs(10){let _=cleanup.kill();let _=cleanup.wait();break;}
 std::thread::sleep(Duration::from_millis(50));}
 }
 }
 result
}
#[cfg(test)]mod tests{
 use super::*;
 fn p()->Policy{Policy{image_id:format!("sha256:{}","a".repeat(64)),gpu_uuid:"GPU-12345678-1234-1234-1234-123456789abc".into(),
 host_bytes:8<<30,threads:4,timeout:Duration::from_secs(60),max_input_bytes:1<<30,
 cloud:fused_ply::Policy{max_points:100,max_bytes:4096,max_abs_coordinate:100.}}}
 #[test]fn rejects_tags_and_wildcard_gpu(){let mut q=p();assert!(valid(&q));q.image_id="colmap:latest".into();assert!(!valid(&q));
 let mut q=p();q.gpu_uuid="all".into();assert!(!valid(&q));}
}
