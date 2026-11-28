//! Experimental Linux CPU-only COLMAP worker using bubblewrap.
//! Not wired into Desktop. Host must qualify installed dependencies, cgroup,
//! disk quota, input manifest, ACL and output validation before production use.
use super::{colmap,Device};
use sha2::{Digest,Sha256};
use std::{fs::{self,File},io::Read,path::{Path,PathBuf},time::{Duration,Instant}};
#[derive(Debug)]
pub enum Error{Unsupported,Invalid,Io(std::io::Error),HashMismatch,Limit,Cancelled,Timeout,Failed(Option<i32>),Plan(colmap::PlanError)}
impl From<std::io::Error> for Error{fn from(e:std::io::Error)->Self{Self::Io(e)}}
#[derive(Debug,Clone)]
pub struct ApprovedPack{pub binary:PathBuf,pub sha256:String}
#[derive(Debug,Clone,Copy)]
pub struct Limits{pub timeout:Duration,pub address_space_bytes:u64,pub max_file_bytes:u64}
fn digest_file(p:&Path)->Result<String,Error>{
 let meta=fs::symlink_metadata(p)?;
 if !meta.is_file()||meta.file_type().is_symlink()||meta.len()==0||meta.len()>1024*1024*1024{return Err(Error::Invalid);}
 let mut f=File::open(p)?;let mut h=Sha256::new();let mut buf=[0;64*1024];let mut n=0u64;
 loop{let k=f.read(&mut buf)?;if k==0{break;}n+=k as u64;if n>meta.len(){return Err(Error::Invalid);}h.update(&buf[..k]);}
 if n!=meta.len(){return Err(Error::Invalid);}Ok(format!("{:x}",h.finalize()))
}
fn verify(pack:&ApprovedPack)->Result<PathBuf,Error>{
 if !pack.binary.is_absolute()||!super::digest(&pack.sha256){return Err(Error::Invalid);}
 if digest_file(&pack.binary)?!=pack.sha256{return Err(Error::HashMismatch);}
 Ok(pack.binary.canonicalize()?)
}
fn valid_limits(l:Limits)->bool{
 !l.timeout.is_zero()&&l.timeout<=Duration::from_secs(86400)&&l.address_space_bytes>=256*1024*1024&&l.max_file_bytes>0
}
/// CPU sparse reconstruction only. All commands are generated internally, never supplied by UI.
/// Linux sandbox requires /usr/bin/bwrap and /usr/bin/prlimit. On unsupported hosts fail closed.
pub fn sparse(root:&Path,pack:&ApprovedPack,config:&colmap::Config,limits:Limits,mut cancelled:impl FnMut()->bool)->Result<(),Error>{
 if config.device!=Device::Cpu||config.gpu_index.is_some(){return Err(Error::Unsupported);}
 if !valid_limits(limits)||!root.is_absolute(){return Err(Error::Invalid);}
 #[cfg(not(target_os="linux"))]{let _=(pack,&mut cancelled);return Err(Error::Unsupported);}
 #[cfg(target_os="linux")]{
 use std::process::{Command,Stdio};
 let binary=verify(pack)?;let root=root.canonicalize()?;
 if root==Path::new("/")||binary.starts_with(&root)||!root.join("images").is_dir()||!root.join("sparse").is_dir()||
 root.join("database.db").exists(){return Err(Error::Invalid);}
 for d in ["images","sparse"]{let m=fs::symlink_metadata(root.join(d))?;if !m.is_dir()||m.file_type().is_symlink(){return Err(Error::Invalid);}}
 if fs::read_dir(root.join("sparse"))?.next().is_some(){return Err(Error::Invalid);}
 let mut count=0u32;
 for item in fs::read_dir(root.join("images"))?{
 let item=item?;let name=item.file_name();let name=name.to_str().ok_or(Error::Invalid)?;
 if !item.file_type()?.is_file()||!super::colmap_model::safe_name(name)||!name.ends_with(".jpg"){return Err(Error::Invalid);}
 count=count.checked_add(1).ok_or(Error::Limit)?;if count>config.image_count{return Err(Error::Invalid);}
 }
 if count!=config.image_count{return Err(Error::Invalid);}
 let layout=colmap::Layout::from_host_root("/work".into()).map_err(Error::Plan)?;
 let plan=colmap::sparse_plan(&layout,config).map_err(Error::Plan)?;
 if !Path::new("/usr/bin/bwrap").is_file()||!Path::new("/usr/bin/prlimit").is_file(){return Err(Error::Unsupported);}
 let started=Instant::now();
 for step in plan{
 if cancelled(){return Err(Error::Cancelled);}if started.elapsed()>=limits.timeout{return Err(Error::Timeout);}
 let mut cmd=Command::new("/usr/bin/bwrap");
 cmd.env_clear().args(["--unshare-all","--new-session","--die-with-parent","--ro-bind","/usr","/usr",
 "--ro-bind-try","/lib","/lib","--ro-bind-try","/lib64","/lib64","--ro-bind-try","/etc/ld.so.cache","/etc/ld.so.cache",
 "--proc","/proc","--dev","/dev","--tmpfs","/tmp","--dir","/home","--setenv","HOME","/home",
 "--setenv","PATH","/usr/bin","--setenv","QT_QPA_PLATFORM","offscreen","--bind"])
 .arg(&root).arg("/work").arg("--ro-bind").arg(&binary).arg("/engine")
 .args(["--chdir","/work","--","/usr/bin/prlimit"])
 .arg(format!("--as={}:{}",limits.address_space_bytes,limits.address_space_bytes))
 .arg(format!("--fsize={}:{}",limits.max_file_bytes,limits.max_file_bytes))
 .args(["--","/engine"]).args(step.args).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null());
 let mut child=cmd.spawn()?;
 loop{
 match child.try_wait(){Ok(Some(s))=>{if !s.success(){return Err(Error::Failed(s.code()));}break;},
 Ok(None)=>{},Err(e)=>{let _=child.kill();let _=child.wait();return Err(Error::Io(e));}}
 if cancelled()||started.elapsed()>=limits.timeout{
 let was_cancelled=started.elapsed()<limits.timeout;let _=child.kill();let _=child.wait();
 return Err(if was_cancelled{Error::Cancelled}else{Error::Timeout});}
 std::thread::sleep(Duration::from_millis(25));
 }
 }
 // Exit status alone is not acceptance: require at least one sparse model.
 let mut found=false;
 for entry in fs::read_dir(root.join("sparse"))?{
 let entry=entry?;if !entry.file_type()?.is_dir(){continue;}
 let p=entry.path();
 if ["cameras.bin","images.bin","points3D.bin"].iter().all(|name|fs::symlink_metadata(p.join(name)).is_ok_and(|m|m.is_file()&&m.len()>0)){found=true;break;}
 }
 if !found{return Err(Error::Invalid);}Ok(())
 }
}
#[cfg(test)]mod tests{
 use super::*;
 #[test]fn rejects_bad_limits_and_unapproved_digest(){
 assert!(!valid_limits(Limits{timeout:Duration::ZERO,address_space_bytes:1,max_file_bytes:1}));
 let p=std::env::temp_dir().join(format!("ovg-pack-{}",uuid::Uuid::new_v4()));
 fs::write(&p,b"binary").unwrap();
 assert!(matches!(verify(&ApprovedPack{binary:p.clone(),sha256:"a".repeat(64)}),Err(Error::HashMismatch)));
 let h=digest_file(&p).unwrap();assert!(verify(&ApprovedPack{binary:p.clone(),sha256:h}).is_ok());let _=fs::remove_file(p);
 }
}
