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
 Ok(Status{available:cfg!(feature="native-cad")&&executable(&root).is_ok(),source_revision:SOURCE,mode:"native editor and conversion sidecar"})
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

#[derive(Clone,Debug,Serialize)]
pub struct EditorStatus{pub session:Uuid,pub state:String,pub format:String,pub changed:bool,pub bytes:u64,pub sha256:String,pub parent:Option<Uuid>}
struct Editor{owner:Uuid,dir:tempfile::TempDir,path:PathBuf,format:String,original:String,parent:Option<Uuid>,child:Option<std::process::Child>,failed:bool}
#[derive(Clone,Default)]
pub struct CadEditors{sessions:std::sync::Arc<std::sync::Mutex<std::collections::HashMap<Uuid,Editor>>>}
fn stage_copy(src:&Path,dest:&Path,fmt:&str)->R<String>{
 drawing(src,fmt)?;
 let mut from=file(src,MAX)?.take(MAX+1);
 let mut to=fs::OpenOptions::new().write(true).create_new(true).open(dest).map_err(|_|"STORAGE_UNAVAILABLE")?;
 let n=std::io::copy(&mut from,&mut to).map_err(|_|"STORAGE_UNAVAILABLE")?;
 if n==0||n>MAX{return Err("FILE_LIMIT")}
 to.sync_all().map_err(|_|"STORAGE_UNAVAILABLE")?;drop(to);
 drawing(dest,fmt)?;Ok(digest(dest,MAX)?.1)
}
impl Editor{
 fn poll(&mut self,id:Uuid)->R<EditorStatus>{
  if let Some(child)=self.child.as_mut(){
   match child.try_wait().map_err(|_|"CAD_PROCESS_FAILED")?{
    None=>return Ok(EditorStatus{session:id,state:"editing".into(),format:self.format.clone(),changed:false,bytes:0,sha256:String::new(),parent:self.parent}),
    Some(s)=>{self.failed=!s.success();self.child=None;}
   }
  }
  drawing(&self.path,&self.format)?;
  let (bytes,sha256)=digest(&self.path,MAX)?;
  Ok(EditorStatus{session:id,state:if self.failed{"recoverable"}else{"ready"}.into(),format:self.format.clone(),changed:sha256!=self.original,bytes,sha256,parent:self.parent})
 }
}
impl CadEditors{
 fn start(&self,owner:Uuid,resource:&Path,private:&Path,src:&Path,parent:Option<Uuid>)->R<EditorStatus>{
  let mut map=self.sessions.lock().map_err(|_|"SESSION_FAILED")?;
  if map.len()>=2||map.values().any(|e|e.owner==owner){return Err("CAD_EDITOR_LIMIT")}
  let fmt=format(src)?;
  let exe=executable(resource)?;
  fs::create_dir_all(private).map_err(|_|"STORAGE_UNAVAILABLE")?;
  if !fs::symlink_metadata(private).map_err(|_|"STORAGE_UNAVAILABLE")?.is_dir(){return Err("INVALID_PRIVATE_ROOT")}
  let dir=tempfile::Builder::new().prefix("cad-edit-").tempdir_in(private).map_err(|_|"STORAGE_UNAVAILABLE")?;
  let path=dir.path().join(format!("drawing.{fmt}"));
  let original=stage_copy(src,&path,fmt)?;
  let child=Command::new(exe).arg("--new-instance").arg(&path).current_dir(dir.path()).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null()).spawn().map_err(|_|"CAD_START_FAILED")?;
  let session=Uuid::new_v4();
  map.insert(session,Editor{owner,dir,path,format:fmt.into(),original,parent,child:Some(child),failed:false});
  Ok(EditorStatus{session,state:"editing".into(),format:fmt.into(),changed:false,bytes:0,sha256:String::new(),parent})
 }
 fn poll(&self,owner:Uuid,id:Uuid)->R<EditorStatus>{
  let mut map=self.sessions.lock().map_err(|_|"SESSION_FAILED")?;
  let e=map.get_mut(&id).filter(|e|e.owner==owner).ok_or("STALE_CAD_SESSION")?;
  e.poll(id)
 }
 fn publish(&self,owner:Uuid,id:Uuid,dest:&Path,expected:&str)->R<Export>{
  let mut map=self.sessions.lock().map_err(|_|"SESSION_FAILED")?;
  let e=map.get_mut(&id).filter(|e|e.owner==owner).ok_or("STALE_CAD_SESSION")?;
  let s=e.poll(id)?;
  if s.state=="editing"{return Err("CAD_EDITOR_STILL_OPEN")}
  if !s.changed{return Err("CAD_DRAWING_UNCHANGED")}
  if s.sha256!=expected{return Err("CAD_DRAWING_CHANGED")}
  if format(dest)?!=e.format||dest.exists(){return Err("INVALID_DESTINATION")}
  let dir=tempfile::tempdir_in(dest.parent().ok_or("INVALID_DESTINATION")?).map_err(|_|"STORAGE_UNAVAILABLE")?;
  let staged=dir.path().join(format!("result.{}",e.format));
  if stage_copy(&e.path,&staged,&e.format)?!=expected{return Err("CAD_DRAWING_CHANGED")}
  fs::hard_link(&staged,dest).map_err(|_|"DESTINATION_UNAVAILABLE")?;
  let result=Export{bytes:s.bytes,sha256:s.sha256,format:s.format};
  map.remove(&id);Ok(result)
 }

 fn adopt(&self,owner:Uuid,id:Uuid,store:&ovg_project_store::Store,revision:u64,expected:&str)->R<ovg_project_store::asset_store::AssetMeta>{
  let mut map=self.sessions.lock().map_err(|_|"SESSION_FAILED")?;
  let e=map.get_mut(&id).filter(|e|e.owner==owner).ok_or("STALE_CAD_SESSION")?;
  let s=e.poll(id)?;
  if s.state=="editing"||s.sha256!=expected||(!s.changed&&e.parent.is_some()){return Err("CAD_DRAWING_NOT_READY")}
  let kind=if e.format=="dwg"{"cad-native-dwg"}else{"cad-native-dxf"};
  let stage=tempfile::tempdir().map_err(|_|"STORAGE_UNAVAILABLE")?;
  let copy=stage.path().join(format!("result.{}",e.format));
  if stage_copy(&e.path,&copy,&e.format)?!=expected{return Err("CAD_DRAWING_CHANGED")}
  let result=store.attach_cad_version(Uuid::new_v4(),kind,&copy,revision,expected,e.parent.map(|p|(p,e.original.as_str())),&AtomicBool::new(false))
   .map_err(|e|match e{ovg_project_store::Error::Conflict=>"REVISION_OR_PARENT_CONFLICT",ovg_project_store::Error::Limit=>"RESOURCE_LIMIT",_=>"CAD_ADOPTION_FAILED"})?;
  map.remove(&id);Ok(result)
 }
 fn discard(&self,owner:Uuid,id:Uuid)->R<()>{
  let mut map=self.sessions.lock().map_err(|_|"SESSION_FAILED")?;
  let e=map.get_mut(&id).filter(|e|e.owner==owner).ok_or("STALE_CAD_SESSION")?;
  if let Some(child)=e.child.as_mut(){
   if child.try_wait().map_err(|_|"CAD_PROCESS_FAILED")?.is_none(){return Err("CAD_EDITOR_STILL_OPEN")}
  }
  map.remove(&id);Ok(())
 }
 // Never kill a running editor or delete unsaved working files during GIS exit.
 // Keep staged copies in private application data for explicit manual recovery.
 pub fn preserve_on_exit(&self){
  if let Ok(mut map)=self.sessions.lock(){for (_,e) in map.drain(){let _=e.dir.keep();}}
 }
}
#[tauri::command]
pub async fn native_cad_edit_open(window:WebviewWindow,u:State<'_,Unified>,editors:State<'_,CadEditors>,handle:Uuid)->Reply<Option<EditorStatus>>{
 #[cfg(feature="native-cad")]{
  trusted(&window)?;let app=window.app_handle().clone();let ed=editors.inner().clone();
  work(window,u.inner().clone(),move|s|{
   s.with(handle,|_|Ok(()))?;
   let root=app.path().resource_dir().map_err(|_|"CAD_RUNTIME_NOT_PACKAGED")?;
   let private=app.path().app_local_data_dir().map_err(|_|"STORAGE_UNAVAILABLE")?.join("cad-recovery");
   executable(&root)?;
   let Some(file)=app.dialog().file().add_filter("CAD DWG / DXF",&["dwg","dxf"]).blocking_pick_file()else{return Ok(None)};
   let path=file.into_path().map_err(|_|"INVALID_FILE")?;
   s.with(handle,|_|ed.start(handle,&root,&private,&path,None).map(Some))
  }).await
 }
 #[cfg(not(feature="native-cad"))]{let _=(u,editors,handle);trusted(&window)?;Err("CAD_RUNTIME_NOT_PACKAGED".into())}
}
#[tauri::command]
pub async fn native_cad_edit_poll(window:WebviewWindow,editors:State<'_,CadEditors>,handle:Uuid,session:Uuid)->Reply<EditorStatus>{
 trusted(&window)?;let ed=editors.inner().clone();
 tauri::async_runtime::spawn_blocking(move||ed.poll(handle,session)).await.map_err(|_|"SESSION_FAILED".to_owned())?.map_err(str::to_owned)
}
#[tauri::command]
pub async fn native_cad_edit_save(window:WebviewWindow,editors:State<'_,CadEditors>,handle:Uuid,session:Uuid,expected:String)->Reply<Option<Export>>{
 trusted(&window)?;let app=window.app_handle().clone();let ed=editors.inner().clone();
 tauri::async_runtime::spawn_blocking(move||->R<Option<Export>>{
  let s=ed.poll(handle,session)?;
  if s.state=="editing"||!s.changed||s.sha256!=expected{return Err("CAD_DRAWING_NOT_READY")}
  let Some(file)=app.dialog().file().add_filter("Edited CAD",&[s.format.as_str()]).set_file_name(format!("edited.{}",s.format)).blocking_save_file()else{return Ok(None)};
  let dest=file.into_path().map_err(|_|"INVALID_DESTINATION")?;
  ed.publish(handle,session,&dest,&expected).map(Some)
 }).await.map_err(|_|"SESSION_FAILED".to_owned())?.map_err(str::to_owned)
}
#[tauri::command]
pub fn native_cad_edit_discard(window:WebviewWindow,editors:State<'_,CadEditors>,handle:Uuid,session:Uuid)->Reply<()>{
 trusted(&window)?;editors.discard(handle,session).map_err(str::to_owned)
}
#[tauri::command]
pub async fn native_cad_edit_list(window:WebviewWindow,editors:State<'_,CadEditors>,handle:Uuid)->Reply<Vec<EditorStatus>>{
 trusted(&window)?;let ed=editors.inner().clone();
 tauri::async_runtime::spawn_blocking(move||->R<Vec<EditorStatus>>{
  let mut map=ed.sessions.lock().map_err(|_|"SESSION_FAILED")?;
  map.iter_mut().filter(|(_,e)|e.owner==handle).map(|(id,e)|e.poll(*id)).collect()
 }).await.map_err(|_|"SESSION_FAILED".to_owned())?.map_err(str::to_owned)
}

#[cfg(test)]
mod editor_tests{
 use super::*;
 fn drawing_fixture(p:&Path,comment:&str){
  fs::write(p,format!("0\nSECTION\n2\nHEADER\n999\n{comment}\n0\nENDSEC\n0\nEOF\n")).unwrap();
 }
 #[test]fn edit_publish_checks_owner_digest_and_no_clobber(){
  let d=tempfile::tempdir().unwrap();let original=d.path().join("original.dxf");
  drawing_fixture(&original,"source");let original_bytes=fs::read(&original).unwrap();
  let stage=tempfile::tempdir_in(d.path()).unwrap();let copy=stage.path().join("drawing.dxf");
  let hash=stage_copy(&original,&copy,"dxf").unwrap();
  drawing_fixture(&copy,"changed");let editors=CadEditors::default();let owner=Uuid::new_v4();let id=Uuid::new_v4();
  editors.sessions.lock().unwrap().insert(id,Editor{owner,dir:stage,path:copy,format:"dxf".into(),original:hash,parent:None,child:None,failed:false});
  let ready=editors.poll(owner,id).unwrap();assert!(ready.changed);assert_eq!(ready.state,"ready");
  assert!(editors.poll(Uuid::new_v4(),id).is_err());
  let dest=d.path().join("edited.dxf");
  assert!(editors.publish(owner,id,&dest,&"0".repeat(64)).is_err());assert!(!dest.exists());
  drawing_fixture(&dest,"existing");let saved=fs::read(&dest).unwrap();
  assert!(editors.publish(owner,id,&dest,&ready.sha256).is_err());assert_eq!(fs::read(&dest).unwrap(),saved);
  let dest=d.path().join("new.dxf");let r=editors.publish(owner,id,&dest,&ready.sha256).unwrap();
  assert_eq!(r.sha256,ready.sha256);assert_eq!(fs::read(&original).unwrap(),original_bytes);assert!(editors.poll(owner,id).is_err());
 }
 #[test]fn exit_retains_staged_drawing(){
  let root=tempfile::tempdir().unwrap();let stage=tempfile::tempdir_in(root.path()).unwrap();let p=stage.path().join("drawing.dxf");drawing_fixture(&p,"recover");
  let ed=CadEditors::default();ed.sessions.lock().unwrap().insert(Uuid::new_v4(),Editor{owner:Uuid::new_v4(),dir:stage,path:p.clone(),format:"dxf".into(),original:"unused".into(),parent:None,child:None,failed:true});
  ed.preserve_on_exit();assert!(p.exists());assert!(ed.sessions.lock().unwrap().is_empty());
 }
}

#[tauri::command]
pub async fn native_cad_edit_project(window:WebviewWindow,u:State<'_,Unified>,editors:State<'_,CadEditors>,handle:Uuid,asset:Uuid,expected:String)->Reply<EditorStatus>{
 #[cfg(feature="native-cad")]{
  trusted(&window)?;let app=window.app_handle().clone();let ed=editors.inner().clone();
  work(window,u.inner().clone(),move|s|s.with(handle,|store|{
   if store.revision().map_err(|_|"STORE_QUERY_FAILED")?!=unified::number(&expected)?{return Err("REVISION_CONFLICT")}
   let meta=store.assets().map_err(|_|"STORE_QUERY_FAILED")?.into_iter().find(|a|a.id==asset).ok_or("STALE_ASSET")?;
   let ext=match meta.kind.as_str(){"cad-native-dwg"=>"dwg","cad-native-dxf"=>"dxf",_=>return Err("UNSUPPORTED_CAD_DOCUMENT")};
   let stage=tempfile::tempdir().map_err(|_|"STORAGE_UNAVAILABLE")?;
   let src=stage.path().join(format!("source.{ext}"));
   store.materialize_asset(asset,&src,&AtomicBool::new(false)).map_err(|_|"CAD_ASSET_READ_FAILED")?;
   let root=app.path().resource_dir().map_err(|_|"CAD_RUNTIME_NOT_PACKAGED")?;
   let private=app.path().app_local_data_dir().map_err(|_|"STORAGE_UNAVAILABLE")?.join("cad-recovery");
   ed.start(handle,&root,&private,&src,Some(asset))
  })).await
 }
 #[cfg(not(feature="native-cad"))]{let _=(u,editors,handle,asset,expected);trusted(&window)?;Err("CAD_RUNTIME_NOT_PACKAGED".into())}
}
#[tauri::command]
pub async fn native_cad_edit_adopt(window:WebviewWindow,u:State<'_,Unified>,editors:State<'_,CadEditors>,handle:Uuid,session:Uuid,revision:String,expected:String)->Reply<ovg_project_store::asset_store::AssetMeta>{
 trusted(&window)?;let ed=editors.inner().clone();
 work(window,u.inner().clone(),move|s|s.with(handle,|store|ed.adopt(handle,session,store,unified::number(&revision)?,&expected))).await
}
#[tauri::command]
pub async fn native_cad_versions(window:WebviewWindow,u:State<'_,Unified>,handle:Uuid)->Reply<Vec<ovg_project_store::asset_store::CadVersion>>{
 work(window,u.inner().clone(),move|s|s.with(handle,|store|store.cad_versions().map_err(|_|"STORE_QUERY_FAILED"))).await
}
