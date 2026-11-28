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
 pub fn ensure_closed(&self,owner:Uuid)->R<()>{
  let map=self.sessions.lock().map_err(|_|"SESSION_FAILED")?;
  if map.values().any(|e|e.owner==owner){return Err("SAVE_OR_DISCARD_CAD_SESSION_FIRST")}Ok(())
 }
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

// Explicit WGS84 read-only projection. This never rewrites the CAD document;
// unsupported entities, Z/OCS or duplicate handles reject the entire snapshot.
fn cad_root(store:&ovg_project_store::Store,asset:Uuid)->R<Uuid>{
 let ancestry=store.cad_versions().map_err(|_|"STORE_QUERY_FAILED")?;
 let mut root=asset;let mut seen=std::collections::HashSet::new();
 while let Some(parent)=ancestry.iter().find(|v|v.asset==root){
  if !seen.insert(root)||seen.len()>32{return Err("INVALID_CAD_ANCESTRY")}root=parent.parent;
 }
 Ok(root)
}
fn cad_feature_id(root:Uuid,handle:&str)->Uuid{
 let mut hash=Sha256::new();hash.update(b"OVG-CAD-ENTITY-1");hash.update(root.as_bytes());hash.update(handle.as_bytes());
 let digest=hash.finalize();let mut raw=[0;16];raw.copy_from_slice(&digest[..16]);raw[6]=(raw[6]&15)|0x80;raw[8]=(raw[8]&63)|0x80;
 Uuid::from_bytes(raw)
}
fn project_dxf(store:&ovg_project_store::Store,asset:Uuid,revision:u64,operation:Uuid,layer:Uuid,name:&str)->R<ovg_project_store::bulk_import::BulkReceipt>{
 use ovg_project_store::{Error,Image,bulk_import::BulkRow};
 if store.revision().map_err(|_|"STORE_QUERY_FAILED")?!=revision{return Err("REVISION_CONFLICT")}
 let a=store.assets().map_err(|_|"STORE_QUERY_FAILED")?.into_iter().find(|a|a.id==asset).ok_or("STALE_ASSET")?;
 if a.kind!="cad-native-dxf"{return Err("DXF_REQUIRED")}
 let dir=tempfile::tempdir().map_err(|_|"STORAGE_UNAVAILABLE")?;
 let path=dir.path().join("source.dxf");
 let cancel=AtomicBool::new(false);
 store.materialize_asset(asset,&path,&cancel).map_err(|_|"CAD_ASSET_READ_FAILED")?;
 let cad=crate::resource_files::Cad::open(&path)?;
 let root=cad_root(store,asset)?;
 let rows=cad.features().iter().map(|v|{
  let handle=v["properties"]["handle"].as_str().ok_or(Error::Invalid)?;
  let id=cad_feature_id(root,handle);
  let g=ovg_spatial::parse_geojson(&v["geometry"],&ovg_spatial::ValidationLimits::default()).map_err(|_|Error::Invalid)?;
  let wkb=ovg_spatial::encode_wkb(&g,None).map_err(|_|Error::Invalid)?;
  Ok(BulkRow{id,image:Image{wkb,attributes:serde_json::json!({"cad_root":root,"cad_asset":asset,"cad_sha256":a.sha256,"cad_handle":handle,"cad_layer":v["properties"]["layer"],"cad_entity":v["properties"]["entity"]})}})
 });
 store.import_layer_stream(operation,layer,name,revision,rows,&cancel).map_err(|e|match e{Error::Conflict=>"REVISION_CONFLICT",Error::Limit=>"RESOURCE_LIMIT",Error::Reused=>"OPERATION_ID_REUSED",_=>"CAD_PROJECTION_FAILED"})
}
#[tauri::command]
pub async fn native_cad_project_dxf(window:WebviewWindow,u:State<'_,Unified>,handle:Uuid,asset:Uuid,expected:String,operation:Uuid,layer:Uuid,name:String,assert_wgs84:bool)->Reply<Value>{
 if !assert_wgs84{return Err("EXPLICIT_CRS_REQUIRED".into())}
 work(window,u.inner().clone(),move|s|s.with(handle,|store|{
  let r=project_dxf(store,asset,unified::number(&expected)?,operation,layer,&name)?;
  Ok(serde_json::json!({"layer":r.layer,"revision":r.revision.to_string(),"count":r.count,"digest":r.digest}))
 })).await
}
#[cfg(test)]
mod projection_tests{
 use super::*;
 #[test]fn project_dxf_preserves_handle_and_source_version(){
  let d=tempfile::tempdir().unwrap();let p=d.path().join("a.dxf");
  fs::write(&p,"0\nSECTION\n2\nENTITIES\n0\nPOINT\n5\nAB\n8\nSurvey\n10\n106\n20\n11\n0\nENDSEC\n0\nEOF\n").unwrap();
  let s=ovg_project_store::Store::create(&d.path().join("p.sqlite")).unwrap();
  let a=Uuid::new_v4();let h=digest(&p,MAX).unwrap().1;
  s.attach_cad_version(a,"cad-native-dxf",&p,0,&h,None,&AtomicBool::new(false)).unwrap();
  let l=Uuid::new_v4();let r=project_dxf(&s,a,1,Uuid::new_v4(),l,"CAD projection").unwrap();
  assert_eq!(r.count,1);let page=s.query(l,[105.,10.,107.,12.],0,10,2).unwrap();
  assert_eq!(page.rows[0].image.attributes["cad_handle"],"AB");assert_eq!(page.rows[0].image.attributes["cad_asset"],a.to_string());
  assert_eq!(s.assets().unwrap()[0].sha256,h);assert!(s.integrity().unwrap());
 }
}

// Narrow embedded editor profile: ASCII DXF POINT/LINE, WGS84 XY, no
// entity creation/deletion or CRS transformation. Exact non-coordinate lines
// remain untouched, including handles, layers, styles and unsupported sections.
fn load_dxf(store:&ovg_project_store::Store,asset:Uuid,revision:u64)->R<(tempfile::TempDir,PathBuf,ovg_project_store::asset_store::AssetMeta,crate::resource_files::Cad)>{
 if store.revision().map_err(|_|"STORE_QUERY_FAILED")?!=revision{return Err("REVISION_CONFLICT")}
 let a=store.assets().map_err(|_|"STORE_QUERY_FAILED")?.into_iter().find(|a|a.id==asset).ok_or("STALE_ASSET")?;
 if a.kind!="cad-native-dxf"||a.bytes>4*1024*1024{return Err("DXF_PROFILE_REQUIRED")}
 let dir=tempfile::tempdir().map_err(|_|"STORAGE_UNAVAILABLE")?;let p=dir.path().join("source.dxf");
 store.materialize_asset(asset,&p,&AtomicBool::new(false)).map_err(|_|"CAD_ASSET_READ_FAILED")?;
 let cad=crate::resource_files::Cad::open(&p)?;Ok((dir,p,a,cad))
}
fn replacement(g:&Value)->R<Vec<(u16,f64)>>{
 let xy=|v:&Value|->R<(f64,f64)>{
  let a=v.as_array().filter(|a|a.len()==2).ok_or("XY_REQUIRED")?;
  let x=a[0].as_f64().filter(|v|v.is_finite()&&v.abs()<=180.).ok_or("INVALID_X")?;
  let y=a[1].as_f64().filter(|v|v.is_finite()&&v.abs()<=90.).ok_or("INVALID_Y")?;Ok((x,y))
 };
 let a=&g["coordinates"];
 match g["type"].as_str(){
  Some("Point")=>{let(x,y)=xy(a)?;Ok(vec![(10,x),(20,y)])},
  Some("LineString")=>{let a=a.as_array().filter(|a|a.len()==2).ok_or("TWO_ENDPOINTS_REQUIRED")?;let(x,y)=xy(&a[0])?;let(u,v)=xy(&a[1])?;if (x,y)==(u,v){return Err("DEGENERATE_LINE")}Ok(vec![(10,x),(20,y),(11,u),(21,v)])},
  _=>Err("UNSUPPORTED_DXF_ENTITY")
 }
}
fn patch_dxf(src:&Path,out:&Path,old:&crate::resource_files::Cad,handle:&str,g:&Value)->R<()>{
 let target=old.features().iter().find(|f|f["properties"]["handle"]==handle).ok_or("STALE_ENTITY")?;
 if target["geometry"]["type"]!=g["type"]{return Err("CAD_ENTITY_TYPE_MISMATCH")}
 let values=replacement(g)?;
 let bytes=fs::read(src).map_err(|_|"STORAGE_UNAVAILABLE")?;
 if bytes.len()>4*1024*1024{return Err("RESOURCE_LIMIT")}
 let raw=std::str::from_utf8(&bytes).map_err(|_|"UNSUPPORTED_DXF")?;
 let lines:Vec<&str>=raw.split_inclusive('\n').collect();
 if lines.len()%2!=0{return Err("INVALID_DXF")}
 let codes:Vec<u16>=lines.chunks_exact(2).map(|p|p[0].trim().parse::<u16>().map_err(|_|"INVALID_DXF")).collect::<R<_>>()?;
 let field=|i:usize|lines[i*2+1].trim();
 let mut found=0;let mut edits=std::collections::HashMap::<usize,String>::new();
 let mut i=0;let mut entities=false;
 while i<codes.len(){
  if codes[i]==0&&field(i)=="SECTION"&&codes.get(i+1)==Some(&2)&&field(i+1)=="ENTITIES"{entities=true;i+=2;continue}
  if codes[i]==0&&field(i)=="ENDSEC"{entities=false;i+=1;continue}
  if !entities{i+=1;continue}
  if codes[i]!=0{return Err("INVALID_DXF")}
  let start=i+1;i=start;while i<codes.len()&&codes[i]!=0{i+=1;}
  if !(start..i).any(|j|codes[j]==5&&field(j).eq_ignore_ascii_case(handle)){continue}
  found+=1;
  for (code,n) in &values{
   let hits:Vec<usize>=(start..i).filter(|j|codes[*j]==*code).collect();if hits.len()!=1{return Err("INVALID_DXF")}
   let at=hits[0]*2+1;let line=lines[at];
   let end=if line.ends_with("\r\n"){"\r\n"}else if line.ends_with('\n'){"\n"}else{""};
   edits.insert(at,format!("{n}{end}"));
  }
 }
 if found!=1{return Err("STALE_ENTITY")}
 let mut b=String::new();for(i,line)in lines.iter().enumerate(){b.push_str(edits.get(&i).map(String::as_str).unwrap_or(line));}
 if b.len()>4*1024*1024{return Err("RESOURCE_LIMIT")}
 fs::write(out,b).map_err(|_|"STORAGE_UNAVAILABLE")?;
 let after=crate::resource_files::Cad::open(out)?;
 if after.features().len()!=old.features().len(){return Err("CAD_FIDELITY_FAILED")}
 for (a,b) in old.features().iter().zip(after.features()){
  if a["properties"]!=b["properties"]{return Err("CAD_FIDELITY_FAILED")}
  if a["properties"]["handle"]==handle{
   if replacement(&b["geometry"])?!=values{return Err("CAD_FIDELITY_FAILED")}
  }else if a!=b{return Err("CAD_FIDELITY_FAILED")}
 }
 Ok(())
}
fn adopt_entity(store:&ovg_project_store::Store,asset:Uuid,revision:u64,expected_sha:&str,handle:&str,g:&Value)->R<ovg_project_store::asset_store::AssetMeta>{
 let (dir,src,a,cad)=load_dxf(store,asset,revision)?;
 if a.sha256!=expected_sha{return Err("CAD_PARENT_DIGEST_CHANGED")}
 let out=dir.path().join("edited.dxf");patch_dxf(&src,&out,&cad,handle,g)?;
 let sha=digest(&out,MAX)?.1;
 store.attach_cad_version(Uuid::new_v4(),"cad-native-dxf",&out,revision,&sha,Some((asset,&a.sha256)),&AtomicBool::new(false)).map_err(|e|match e{ovg_project_store::Error::Conflict=>"REVISION_OR_PARENT_CONFLICT",ovg_project_store::Error::Limit=>"RESOURCE_LIMIT",_=>"CAD_ADOPTION_FAILED"})
}
#[tauri::command]
pub async fn native_cad_canvas(window:WebviewWindow,u:State<'_,Unified>,handle:Uuid,asset:Uuid,expected:String,assert_wgs84:bool)->Reply<Value>{
 if !assert_wgs84{return Err("EXPLICIT_CRS_REQUIRED".into())}
 work(window,u.inner().clone(),move|s|s.with(handle,|store|{
  let(_dir,_path,a,cad)=load_dxf(store,asset,unified::number(&expected)?)?;
  if cad.features().len()>512{return Err("CANVAS_ENTITY_LIMIT")}
  let result=serde_json::json!({"sha256":a.sha256,"features":cad.features()});
  if serde_json::to_vec(&result).map_err(|_|"INVALID_DXF")?.len()>1024*1024{return Err("RESOURCE_LIMIT")}Ok(result)
 })).await
}
#[tauri::command]
pub async fn native_cad_canvas_commit(window:WebviewWindow,u:State<'_,Unified>,handle:Uuid,asset:Uuid,expected:String,sha256:String,entity:String,geometry:Value,assert_wgs84:bool)->Reply<ovg_project_store::asset_store::AssetMeta>{
 if !assert_wgs84{return Err("EXPLICIT_CRS_REQUIRED".into())}
 work(window,u.inner().clone(),move|s|s.with(handle,|store|adopt_entity(store,asset,unified::number(&expected)?,&sha256,&entity,&geometry))).await
}
#[tauri::command]
pub async fn native_cad_gis_to_dxf(window:WebviewWindow,u:State<'_,Unified>,handle:Uuid,layer:Uuid,feature:Uuid,expected:String,assert_wgs84:bool)->Reply<ovg_project_store::asset_store::AssetMeta>{
 if !assert_wgs84{return Err("EXPLICIT_CRS_REQUIRED".into())}
 work(window,u.inner().clone(),move|s|s.with(handle,|store|{
  let rev=unified::number(&expected)?;
  let(_,image)=store.feature(layer,feature,rev).map_err(|_|"STALE_FEATURE")?;
  let props=&image.attributes;
  let attr=|k:&str|props[k].as_str().ok_or("CAD_PROVENANCE_REQUIRED");
  let asset=Uuid::parse_str(attr("cad_asset")?).map_err(|_|"CAD_PROVENANCE_REQUIRED")?;
  let root=cad_root(store,asset)?;let entity=attr("cad_handle")?;
  if attr("cad_root")?!=root.to_string()||cad_feature_id(root,entity)!=feature{return Err("CAD_PROVENANCE_MISMATCH")}
  let parsed=ovg_spatial::parse_xy_wkb(&image.wkb,&ovg_spatial::ValidationLimits::default()).map_err(|_|"INVALID_GEOMETRY")?;
  let g=ovg_spatial::to_geojson(&parsed.geometry);
  adopt_entity(store,asset,rev,attr("cad_sha256")?,entity,&g)
 })).await
}
#[cfg(test)]mod canvas_tests{
 use super::*;
 #[test]fn coordinate_patch_preserves_non_coordinate_records(){
  let d=tempfile::tempdir().unwrap();let src=d.path().join("a.dxf");let dst=d.path().join("b.dxf");
  let original="0\r\nSECTION\r\n2\r\nENTITIES\r\n0\r\nLINE\r\n5\r\nAB\r\n8\r\nSurvey\r\n62\r\n3\r\n10\r\n106\r\n20\r\n11\r\n11\r\n107\r\n21\r\n12\r\n0\r\nENDSEC\r\n0\r\nEOF\r\n";
  fs::write(&src,original).unwrap();let cad=crate::resource_files::Cad::open(&src).unwrap();
  let g=serde_json::json!({"type":"LineString","coordinates":[[106.5,11.5],[107.,12.]]});
  patch_dxf(&src,&dst,&cad,"AB",&g).unwrap();
  let edited=fs::read_to_string(dst).unwrap();assert_eq!(edited,original.replace("106\r\n","106.5\r\n").replace("20\r\n11\r\n","20\r\n11.5\r\n"));assert_eq!(fs::read_to_string(src).unwrap(),original);
 }
}

#[tauri::command]
pub async fn native_cad_preview(window:WebviewWindow,u:State<'_,Unified>,handle:Uuid,asset:Uuid,expected:String)->Reply<Value>{
 work(window,u.inner().clone(),move|s|s.with(handle,|store|{
  if store.revision().map_err(|_|"STORE_QUERY_FAILED")?!=unified::number(&expected)?{return Err("REVISION_CONFLICT")}
  let a=store.assets().map_err(|_|"STORE_QUERY_FAILED")?.into_iter().find(|a|a.id==asset).ok_or("STALE_ASSET")?;
  if a.kind!="cad-native-dxf"||a.bytes>4*1024*1024{return Err("DXF_PROFILE_REQUIRED")}
  let dir=tempfile::tempdir().map_err(|_|"STORAGE_UNAVAILABLE")?;let p=dir.path().join("preview.dxf");
  store.materialize_asset(asset,&p,&AtomicBool::new(false)).map_err(|_|"CAD_ASSET_READ_FAILED")?;
  let strokes=crate::cad_preview::open(&p)?;
  let v=serde_json::json!({"sha256":a.sha256,"strokes":strokes,"coordinate_system":"native-unassigned"});
  if serde_json::to_vec(&v).map_err(|_|"INVALID_DXF")?.len()>4*1024*1024{return Err("RESOURCE_LIMIT")}Ok(v)
 })).await
}

// Explicit layer synchronization, bounded to one atomic GIS batch. No silent
// overwrite of independently edited features, no implicit coordinate transforms.
fn cad_wkb(g:&Value)->R<Vec<u8>>{
 let g=ovg_spatial::parse_geojson(g,&ovg_spatial::ValidationLimits::default()).map_err(|_|"INVALID_GEOMETRY")?;
 ovg_spatial::encode_wkb(&g,None).map_err(|_|"INVALID_GEOMETRY")
}
fn projection_rows(store:&ovg_project_store::Store,layer:Uuid,asset:Uuid,revision:u64,meta:&ovg_project_store::asset_store::AssetMeta,cad:&crate::resource_files::Cad)->R<Vec<ovg_project_store::Row>>{
 if cad.features().len()>64{return Err("SYNC_BATCH_LIMIT")}
 let page=store.query(layer,[-180.,-90.,180.,90.],0,65,revision).map_err(|_|"STORE_QUERY_FAILED")?;
 if page.next.is_some()||page.rows.len()!=cad.features().len(){return Err("SYNC_ENTITY_SET_CHANGED")}
 let root=cad_root(store,asset)?;
 for row in &page.rows{
  let p=&row.image.attributes;let h=p["cad_handle"].as_str().ok_or("CAD_PROVENANCE_REQUIRED")?;
  let base=cad.features().iter().find(|f|f["properties"]["handle"]==h).ok_or("SYNC_ENTITY_SET_CHANGED")?;
  if cad_feature_id(root,h)!=row.id||p["cad_root"]!=root.to_string()||p["cad_asset"]!=asset.to_string()||p["cad_sha256"]!=meta.sha256||p["cad_layer"]!=base["properties"]["layer"]||p["cad_entity"]!=base["properties"]["entity"]{return Err("CAD_PROVENANCE_MISMATCH")}
 }Ok(page.rows)
}
fn sync_cad_to_gis(store:&ovg_project_store::Store,layer:Uuid,asset:Uuid,child:Uuid,revision:u64,operation:Uuid)->R<ovg_project_store::Receipt>{
 use ovg_project_store::{Batch,Edit,Image};
 if !store.cad_versions().map_err(|_|"STORE_QUERY_FAILED")?.iter().any(|v|v.asset==child&&v.parent==asset){return Err("DIRECT_CHILD_REQUIRED")}
 let(_a,_p,meta,base)=load_dxf(store,asset,revision)?;
 let(_b,_q,next,updated)=load_dxf(store,child,revision)?;
 let rows=projection_rows(store,layer,asset,revision,&meta,&base)?;
 if updated.features().len()!=rows.len(){return Err("SYNC_ENTITY_SET_CHANGED")}
 let mut edits=Vec::new();
 for row in rows{
  let h=row.image.attributes["cad_handle"].as_str().ok_or("CAD_PROVENANCE_REQUIRED")?;
  let before=base.features().iter().find(|f|f["properties"]["handle"]==h).ok_or("STALE_ENTITY")?;
  let after=updated.features().iter().find(|f|f["properties"]["handle"]==h).ok_or("SYNC_ENTITY_SET_CHANGED")?;
  if before["properties"]["entity"]!=after["properties"]["entity"]{return Err("SYNC_ENTITY_TYPE_CHANGED")}
  if row.image.wkb!=cad_wkb(&before["geometry"])?{return Err("GIS_GEOMETRY_CONFLICT")}
  let mut attrs=row.image.attributes.clone();attrs["cad_asset"]=serde_json::json!(child);attrs["cad_sha256"]=serde_json::json!(next.sha256);attrs["cad_layer"]=after["properties"]["layer"].clone();
  edits.push(Edit{layer,feature:row.id,expected:row.revision,image:Some(Image{wkb:cad_wkb(&after["geometry"])?,attributes:attrs})});
 }
 store.apply(&Batch{operation,expected_project:revision,edits,links:vec![]}).map_err(|e|match e{ovg_project_store::Error::Conflict=>"REVISION_CONFLICT",ovg_project_store::Error::Limit=>"SYNC_BATCH_LIMIT",_=>"SYNC_FAILED"})
}
fn sync_gis_to_cad(store:&ovg_project_store::Store,layer:Uuid,asset:Uuid,revision:u64)->R<ovg_project_store::asset_store::AssetMeta>{
 let(dir,src,meta,cad)=load_dxf(store,asset,revision)?;
 let rows=projection_rows(store,layer,asset,revision,&meta,&cad)?;
 let mut path=src;let mut changed=0;
 for row in rows{
  let h=row.image.attributes["cad_handle"].as_str().ok_or("CAD_PROVENANCE_REQUIRED")?;
  let before=cad.features().iter().find(|f|f["properties"]["handle"]==h).ok_or("STALE_ENTITY")?;
  if row.image.wkb==cad_wkb(&before["geometry"])?{continue}
  let g=ovg_spatial::parse_xy_wkb(&row.image.wkb,&ovg_spatial::ValidationLimits::default()).map_err(|_|"INVALID_GEOMETRY")?;
  let g=ovg_spatial::to_geojson(&g.geometry);
  let current=crate::resource_files::Cad::open(&path)?;
  let out=dir.path().join(format!("sync-{changed}.dxf"));patch_dxf(&path,&out,&current,h,&g)?;path=out;changed+=1;
 }
 if changed==0{return Err("CAD_DRAWING_UNCHANGED")}
 let sha=digest(&path,MAX)?.1;
 store.attach_cad_version(Uuid::new_v4(),"cad-native-dxf",&path,revision,&sha,Some((asset,&meta.sha256)),&AtomicBool::new(false)).map_err(|_|"CAD_ADOPTION_FAILED")
}
#[tauri::command]
pub async fn native_cad_sync_layer(window:WebviewWindow,u:State<'_,Unified>,handle:Uuid,layer:Uuid,asset:Uuid,child:Option<Uuid>,expected:String,operation:Uuid,assert_wgs84:bool)->Reply<Value>{
 if !assert_wgs84{return Err("EXPLICIT_CRS_REQUIRED".into())}
 work(window,u.inner().clone(),move|s|s.with(handle,|store|{
  let rev=unified::number(&expected)?;
  if let Some(c)=child{let r=sync_cad_to_gis(store,layer,asset,c,rev,operation)?;Ok(serde_json::json!({"revision":r.revision.to_string(),"features":r.features.len()}))}
  else{let r=sync_gis_to_cad(store,layer,asset,rev)?;Ok(serde_json::json!({"asset":r.id,"sha256":r.sha256}))}
 })).await
}
