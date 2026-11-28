//! Optional native OpenVietCAD sidecar. Fixed packaged executable, no shell,
//! arbitrary command line, renderer-supplied executable or external URL.
use super::*;
use unified::{Unified,work};
use std::{fs,io::Read,path::{Path,PathBuf},process::{Command,Stdio},time::{Duration,Instant}};
use sha2::{Sha256,Digest};
use serde::{Deserialize,Serialize};
const SOURCE:&str="3e738a2447d0cccf196f3b0f81823cd6a3e1d62d";
const MAX:u64=25*1024*1024;
type R<T>=Result<T,&'static str>;
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Manifest{source_revision:String,sha256:String}
#[derive(Serialize)]
pub struct Status{pub available:bool,pub source_revision:&'static str,pub mode:&'static str}
#[derive(Serialize)]
pub struct Export{pub bytes:u64,pub sha256:String,pub format:String}
fn file(p:&Path,max:u64)->R<fs::File>{
 let m=fs::symlink_metadata(p).map_err(|_|"FILE_UNAVAILABLE")?;
 if !m.file_type().is_file()||m.len()==0||m.len()>max{return Err("FILE_LIMIT")}
 fs::File::open(p).map_err(|_|"FILE_UNAVAILABLE")
}
fn digest(p:&Path,max:u64)->R<(u64,String)>{
 let mut f=file(p,max)?;let mut h=Sha256::new();let mut size=0;let mut b=[0u8;65536];
 loop{let n=f.read(&mut b).map_err(|_|"FILE_UNAVAILABLE")?;if n==0{break}size+=n as u64;if size>max{return Err("FILE_LIMIT")}h.update(&b[..n]);}
 Ok((size,format!("{:x}",h.finalize())))
}
fn executable(root:&Path)->R<PathBuf>{
 let dir=root.join("native-cad");
 if !fs::symlink_metadata(&dir).map_err(|_|"CAD_RUNTIME_NOT_PACKAGED")?.file_type().is_dir(){return Err("CAD_RUNTIME_NOT_PACKAGED")}
 let p=dir.join(if cfg!(windows){"OpenCADStudio.exe"}else{"OpenCADStudio"});
 let mut raw=Vec::new();file(&dir.join("manifest.json"),4096)?.take(4097).read_to_end(&mut raw).map_err(|_|"CAD_MANIFEST_INVALID")?;
 if raw.len()>4096{return Err("CAD_MANIFEST_INVALID")}
 let m:Manifest=serde_json::from_slice(&raw).map_err(|_|"CAD_MANIFEST_INVALID")?;
 if m.source_revision!=SOURCE||m.sha256.len()!=64||!m.sha256.bytes().all(|b|b.is_ascii_hexdigit()&&!b.is_ascii_uppercase()){return Err("CAD_MANIFEST_INVALID")}
 if digest(&p,512*1024*1024)?.1!=m.sha256{return Err("CAD_BINARY_DIGEST_MISMATCH")}
 Ok(p)
}
fn format(path:&Path)->R<&'static str>{match path.extension().and_then(|s|s.to_str()).map(str::to_ascii_lowercase).as_deref(){Some("dxf")=>Ok("dxf"),Some("dwg")=>Ok("dwg"),_=>Err("CAD_FORMAT_UNSUPPORTED")}}
fn drawing(path:&Path,fmt:&str)->R<()>{
 let mut f=file(path,MAX)?;let mut h=[0;22];let n=f.read(&mut h).map_err(|_|"FILE_UNAVAILABLE")?;
 if fmt=="dwg"{if n<6||&h[..4]!=b"AC10"||!h[4..6].iter().all(u8::is_ascii_digit){return Err("INVALID_DWG")}}
 else if fmt=="dxf"{if !h[..n].starts_with(b"AutoCAD Binary DXF")&&!std::str::from_utf8(&h[..n]).ok().is_some_and(|s|s.trim_start().starts_with('0')){return Err("INVALID_DXF")}}
 else{return Err("CAD_FORMAT_UNSUPPORTED")}Ok(())
}
/// Conversion uses the actual native CAD engine's --export IN OUT contract.
/// No --allow-lossy, scripts, --serve or --mcp mode.
fn convert(root:&Path,src:&Path,dest:&Path,cancel:&AtomicBool)->R<Export>{
 if dest.exists(){return Err("DESTINATION_EXISTS")}
 let input_format=format(src)?;let output_format=format(dest)?;
 if input_format==output_format{return Err("SELECT_DIFFERENT_CAD_FORMAT")}
 drawing(src,input_format)?;
 let exe=executable(root)?;
 let stage=tempfile::tempdir_in(dest.parent().ok_or("INVALID_DESTINATION")?).map_err(|_|"STORAGE_UNAVAILABLE")?;
 let input=stage.path().join(format!("input.{input_format}"));let output=stage.path().join(format!("output.{output_format}"));
 let mut from=file(src,MAX)?.take(MAX+1);let mut to=fs::OpenOptions::new().create_new(true).write(true).open(&input).map_err(|_|"STORAGE_UNAVAILABLE")?;
 let bytes=std::io::copy(&mut from,&mut to).map_err(|_|"STORAGE_UNAVAILABLE")?;if bytes>MAX{return Err("FILE_LIMIT")}to.sync_all().map_err(|_|"STORAGE_UNAVAILABLE")?;drop(to);
 drawing(&input,input_format)?;
 if cancel.load(Ordering::Acquire){return Err("CANCELLED")}
 let mut child=Command::new(exe).arg("--export").arg(&input).arg(&output).current_dir(stage.path()).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null()).spawn().map_err(|_|"CAD_START_FAILED")?;
 let deadline=Instant::now()+Duration::from_secs(120);
 let status=loop{
  if cancel.load(Ordering::Acquire)||Instant::now()>deadline{let _=child.kill();let _=child.wait();return Err("CAD_CANCELLED_OR_TIMEOUT")}
  match child.try_wait(){Ok(Some(s))=>break s,Ok(None)=>std::thread::sleep(Duration::from_millis(100)),Err(_)=>{let _=child.kill();let _=child.wait();return Err("CAD_PROCESS_FAILED")}}
 };
 if !status.success(){return Err(if status.code()==Some(3){"CAD_LOSSY_EXPORT_REFUSED"}else{"CAD_CONVERSION_FAILED"})}
 drawing(&output,output_format)?;let (bytes,sha256)=digest(&output,MAX)?;
 fs::File::open(&output).and_then(|f|f.sync_all()).map_err(|_|"STORAGE_UNAVAILABLE")?;
 if cancel.load(Ordering::Acquire){return Err("CANCELLED")}
 fs::hard_link(&output,dest).map_err(|_|"DESTINATION_UNAVAILABLE")?;
 Ok(Export{bytes,sha256,format:output_format.into()})
}
#[tauri::command]
pub fn native_cad_status(window:WebviewWindow)->Reply<Status>{
 trusted(&window)?;
 let root=window.app_handle().path().resource_dir().map_err(|_|"CAD_RUNTIME_NOT_PACKAGED")?;
 Ok(Status{available:cfg!(feature="native-cad")&&executable(&root).is_ok(),source_revision:SOURCE,mode:"separate native conversion sidecar"})
}
#[tauri::command]
pub async fn native_cad_convert(window:WebviewWindow,u:State<'_,Unified>,handle:Uuid,request_id:Uuid)->Reply<Option<Export>>{
 #[cfg(feature="native-cad")]{
  trusted(&window)?;let read=ReadPermit::acquire(request_id)?;
  let app=window.app_handle().clone();
  work(window,u.inner().clone(),move|s|{
   s.with(handle,|_|Ok(()))?;
   let root=app.path().resource_dir().map_err(|_|"CAD_RUNTIME_NOT_PACKAGED")?;
   executable(&root)?;
   let Some(file)=app.dialog().file().add_filter("CAD DWG / DXF",&["dwg","dxf"]).blocking_pick_file() else{return Ok(None)};
   let src=file.into_path().map_err(|_|"INVALID_FILE")?;
   let ext=if format(&src)?=="dwg"{"dxf"}else{"dwg"};
   let Some(file)=app.dialog().file().add_filter("Converted CAD",&[ext]).set_file_name(format!("converted.{ext}")).blocking_save_file() else{return Ok(None)};
   let dest=file.into_path().map_err(|_|"INVALID_DESTINATION")?;
   if format(&dest)?!=ext{return Err("CAD_FORMAT_UNSUPPORTED")}
   s.with(handle,|_|Ok(()))?;
   convert(&root,&src,&dest,&read.token.flag).map(Some)
  }).await
 }
 #[cfg(not(feature="native-cad"))]{let _=(u,handle,request_id);trusted(&window)?;Err("CAD_RUNTIME_NOT_PACKAGED".into())}
}
#[cfg(test)]mod tests{
 use super::*;
 #[test]fn fails_closed_without_sidecar(){let d=tempfile::tempdir().unwrap();assert!(executable(d.path()).is_err());assert!(format(Path::new("a.exe")).is_err());}
 #[test]fn refuses_arbitrary_dwg(){let d=tempfile::tempdir().unwrap();let p=d.path().join("bad.dwg");fs::write(&p,b"not a drawing").unwrap();assert!(drawing(&p,"dwg").is_err());}
}
