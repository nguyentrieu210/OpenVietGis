//! Experimental Linux offline export workers. Trusted host paths and approved packs only.
//! Per-process limits do not enforce aggregate disk/RAM quotas.
use super::{local_worker::{self,ApprovedPack,Limits},mosaic_export,disk_store,ortho::Grid};
use std::{fs::{self,File},io::{Read,Write},path::{Path,PathBuf},ffi::OsString};
#[derive(Debug)]pub enum Error{Invalid,Unsupported,Limit,Conflict,Cancelled,Timeout,Failed,Io(std::io::Error),Worker(local_worker::Error),Mosaic(mosaic_export::Error),Json(serde_json::Error),Store(disk_store::Error)}
impl From<std::io::Error> for Error{fn from(e:std::io::Error)->Self{Self::Io(e)}}
impl From<serde_json::Error> for Error{fn from(e:serde_json::Error)->Self{Self::Json(e)}}
#[derive(Debug,Clone,Copy,PartialEq,Eq)]
pub enum CloudFormat{Laz,Copc}
#[derive(Debug,Clone)]
pub struct Receipt{pub path:PathBuf,pub bytes:u64,pub sha256:String}
fn receipt(p:&Path,max:u64)->Result<Receipt,Error>{
 use sha2::{Sha256,Digest};
 let m=fs::symlink_metadata(p)?;if !m.is_file()||m.file_type().is_symlink()||m.len()==0||m.len()>max{return Err(Error::Limit);}
 let mut f=File::open(p)?;let mut h=Sha256::new();let mut buf=[0u8;65536];let mut n=0u64;
 loop{let k=f.read(&mut buf)?;if k==0{break;}n+=k as u64;if n>m.len(){return Err(Error::Invalid);}h.update(&buf[..k]);}
 if n!=m.len(){return Err(Error::Invalid);}f.sync_all()?;
 Ok(Receipt{path:p.into(),bytes:n,sha256:format!("{:x}",h.finalize())})
}
/// Output directory is new and distinct from read-only input staging.
#[cfg(target_os="linux")]
fn execute(input:&Path,output:&Path,pack:&ApprovedPack,args:&[OsString],limits:Limits,
 log:&str,mut cancelled:impl FnMut()->bool)->Result<(),Error>{
 use std::{process::{Command,Stdio},time::Instant};
 if !local_worker::valid_limits(limits)||!Path::new("/usr/bin/bwrap").is_file()||!Path::new("/usr/bin/prlimit").is_file(){return Err(Error::Unsupported);}
 let binary=local_worker::verify(pack).map_err(Error::Worker)?;
 let i=input.canonicalize()?;let o=output.canonicalize()?;
 if i==Path::new("/")||o==Path::new("/")||i.starts_with(&o)||o.starts_with(&i)||binary.starts_with(&o){return Err(Error::Invalid);}
 if cancelled(){return Err(Error::Cancelled);}
 let stdout=File::options().write(true).create_new(true).open(o.join(log))?;
 let mut cmd=Command::new("/usr/bin/bwrap");
 cmd.env_clear().args(["--unshare-all","--new-session","--die-with-parent","--ro-bind","/usr","/usr",
 "--ro-bind-try","/lib","/lib","--ro-bind-try","/lib64","/lib64","--ro-bind-try","/etc/ld.so.cache","/etc/ld.so.cache",
 "--proc","/proc","--dev","/dev","--tmpfs","/tmp","--dir","/home","--setenv","HOME","/home",
 "--setenv","PATH","/usr/bin","--setenv","PROJ_NETWORK","OFF","--setenv","GDAL_PAM_ENABLED","NO","--ro-bind"])
 .arg(i).arg("/input").arg("--bind").arg(o).arg("/output").arg("--ro-bind").arg(binary).arg("/engine")
 .args(["--chdir","/input","--","/usr/bin/prlimit"])
 .arg(format!("--as={}:{}",limits.address_space_bytes,limits.address_space_bytes))
 .arg(format!("--fsize={}:{}",limits.max_file_bytes,limits.max_file_bytes))
 .args(["--","/engine"]).args(args).stdin(Stdio::null()).stdout(Stdio::from(stdout)).stderr(Stdio::null());
 let mut child=cmd.spawn()?;let started=Instant::now();
 loop{
 match child.try_wait(){Ok(Some(s))=>return if s.success(){Ok(())}else{Err(Error::Failed)},
 Ok(None)=>{},Err(e)=>{let _=child.kill();let _=child.wait();return Err(Error::Io(e));}}
 let cancel=cancelled();if cancel||started.elapsed()>=limits.timeout{let _=child.kill();let _=child.wait();return Err(if cancel{Error::Cancelled}else{Error::Timeout});}
 std::thread::sleep(std::time::Duration::from_millis(25));
 }
}
fn validate_cog(info:&serde_json::Value,g:Grid)->Result<(),Error>{
 if info["driverShortName"]!="GTiff"||info["metadata"]["IMAGE_STRUCTURE"]["LAYOUT"]!="COG"||
 info["size"]!=serde_json::json!([g.width,g.height]){return Err(Error::Invalid);}
 let gt=info["geoTransform"].as_array().ok_or(Error::Invalid)?;
 if gt.len()!=6{return Err(Error::Invalid);}
 for(v,e)in gt.iter().zip([g.origin_x,g.gsd,0.,g.origin_y,0.,-g.gsd]){
 let v=v.as_f64().ok_or(Error::Invalid)?;if !v.is_finite()||(v-e).abs()>g.gsd*1e-6{return Err(Error::Invalid);}
 }
 let bands=info["bands"].as_array().ok_or(Error::Invalid)?;
 if bands.len()!=4||bands.iter().zip(["Red","Green","Blue","Alpha"]).any(|(b,c)|b["type"]!="Byte"||b["colorInterpretation"]!=c){return Err(Error::Invalid);}
 Ok(())
}
/// Runs gdal_translate and gdalinfo through separate approved binaries.
/// Receipt is a candidate artifact, NOT independent structural/CRS qualification.
pub fn cog(store:&disk_store::Store,revision:u64,g:Grid,staging:&Path,output:&Path,
 translate:&ApprovedPack,info:&ApprovedPack,limits:Limits,block:u16,threads:u8,
 mut cancelled:impl FnMut()->bool)->Result<Receipt,Error>{
 #[cfg(not(target_os="linux"))]{let _=(store,revision,g,staging,output,translate,info,limits,block,threads,&mut cancelled);Err(Error::Unsupported)}
 #[cfg(target_os="linux")]{
 if !staging.is_absolute()||!output.is_absolute()||staging==output||!local_worker::valid_limits(limits){return Err(Error::Invalid);}
 let r=store.revision(revision).map_err(Error::Store)?;
 let bytes=r.tiles.values().try_fold(0u64,|n,t|n.checked_add(t.bytes)).ok_or(Error::Limit)?;
 if bytes>limits.input_bytes{return Err(Error::Limit);}
 let mut args=mosaic_export::cog_args(block,threads).map_err(Error::Mosaic)?;
 let last=args.last_mut().ok_or(Error::Invalid)?;*last="/output/mosaic.cog.tif".into();
 mosaic_export::stage_vrt(store,revision,g,staging,64*1024*1024).map_err(Error::Mosaic)?;
 fs::create_dir(output)?;
 execute(staging,output,translate,&args,limits,"translate.log",&mut cancelled)?;
 let artifact=output.join("mosaic.cog.tif");
 let result=receipt(&artifact,limits.max_file_bytes)?;
 // gdalinfo reads candidate output via the writable mount, no VRT input modification.
 execute(staging,output,info,&["-json".into(),"/output/mosaic.cog.tif".into()],limits,"info.json",&mut cancelled)?;
 let data=disk_store::bounded_read(&output.join("info.json"),16*1024*1024).map_err(Error::Store)?;
 validate_cog(&serde_json::from_slice(&data)?,g)?;
 let final_receipt=receipt(&artifact,limits.max_file_bytes)?;
 if final_receipt.sha256!=result.sha256{return Err(Error::Invalid);}Ok(final_receipt)
 }
}
pub fn pdal_pipeline(format:CloudFormat)->serde_json::Value{
 let (name,writer)=match format{CloudFormat::Laz=>("cloud.laz","writers.las"),CloudFormat::Copc=>("cloud.copc.laz","writers.copc")};
 let mut stage=serde_json::json!({"type":writer,"filename":format!("/output/{name}"),"forward":"all"});
 if format==CloudFormat::Laz{stage["compression"]=serde_json::json!(true);}
 serde_json::json!({"pipeline":[{"type":"readers.las","filename":"/input/cloud.las"},stage]})
}
/// Basic LAS header checks; COPC hierarchy and LAZ data require a qualified independent reader.
fn validate_cloud(p:&Path,format:CloudFormat,expected_count:u64)->Result<(),Error>{
 let mut f=File::open(p)?;let mut h=[0u8;227];f.read_exact(&mut h)?;
 if &h[..4]!=b"LASF"||h[104]&0x80==0{return Err(Error::Invalid);}
 let legacy=u32::from_le_bytes(h[107..111].try_into().map_err(|_|Error::Invalid)?) as u64;
 let header=u16::from_le_bytes([h[94],h[95]]);
 let count=if h[25]>=4{
 if header<375{return Err(Error::Invalid);}let mut extra=[0u8;148];f.read_exact(&mut extra)?;
 let n=u64::from_le_bytes(extra[20..28].try_into().map_err(|_|Error::Invalid)?);if n==0{legacy}else{n}
 }else{legacy};
 if count!=expected_count{return Err(Error::Invalid);}
 if format==CloudFormat::Copc{
 if h[25]!=4||header!=375||!matches!(h[104]&0x3f,6|7|8){return Err(Error::Invalid);}
 let mut vlr=[0u8;54];f.read_exact(&mut vlr)?;
 if &vlr[2..6]!=b"copc"||vlr[6..18].iter().any(|b|*b!=0)||u16::from_le_bytes([vlr[18],vlr[19]])!=1||u16::from_le_bytes([vlr[20],vlr[21]])!=160{return Err(Error::Invalid);}
 }
 Ok(())
}
pub fn point_cloud(las:&[u8],expected_count:u64,format:CloudFormat,staging:&Path,output:&Path,
 pack:&ApprovedPack,limits:Limits,mut cancelled:impl FnMut()->bool)->Result<Receipt,Error>{
 #[cfg(not(target_os="linux"))]{let _=(las,expected_count,format,staging,output,pack,limits,&mut cancelled);Err(Error::Unsupported)}
 #[cfg(target_os="linux")]{
 if !staging.is_absolute()||!output.is_absolute()||!local_worker::valid_limits(limits)||las.len()<227||
 las.len() as u64>limits.input_bytes||&las[..4]!=b"LASF"||expected_count==0{return Err(Error::Invalid);}
 fs::create_dir(staging)?;
 let mut f=File::options().write(true).create_new(true).open(staging.join("cloud.las"))?;f.write_all(las)?;f.sync_all()?;
 let mut f=File::options().write(true).create_new(true).open(staging.join("pipeline.json"))?;f.write_all(&serde_json::to_vec(&pdal_pipeline(format))?)?;f.sync_all()?;
 fs::create_dir(output)?;
 execute(staging,output,pack,&["pipeline".into(),"pipeline.json".into()],limits,"pdal.log",&mut cancelled)?;
 let p=output.join(if format==CloudFormat::Copc{"cloud.copc.laz"}else{"cloud.laz"});
 validate_cloud(&p,format,expected_count)?;receipt(&p,limits.max_file_bytes)
 }
}
#[cfg(test)]mod tests{
 use super::*;
 #[test]fn cog_metadata_gate(){
 let g=Grid{origin_x:1.,origin_y:2.,gsd:0.5,width:2,height:3};
 let mut j=serde_json::json!({"driverShortName":"GTiff","size":[2,3],"geoTransform":[1.,0.5,0.,2.,0.,-0.5],
 "metadata":{"IMAGE_STRUCTURE":{"LAYOUT":"COG"}},"bands":[{"type":"Byte","colorInterpretation":"Red"},{"type":"Byte","colorInterpretation":"Green"},{"type":"Byte","colorInterpretation":"Blue"},{"type":"Byte","colorInterpretation":"Alpha"}]});
 assert!(validate_cog(&j,g).is_ok());j["size"]=serde_json::json!([3,2]);assert!(validate_cog(&j,g).is_err());
 }
 #[test]fn fixed_pdal_paths(){let p=pdal_pipeline(CloudFormat::Copc);assert_eq!(p["pipeline"][1]["type"],"writers.copc");assert_eq!(p["pipeline"][1]["filename"],"/output/cloud.copc.laz");}
}
