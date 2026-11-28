//! Local photo preflight via the existing trusted desktop IPC and native file picker.
//! No raw paths or GPS coordinates returned to frontend. No engine execution enabled.
use crate::{host::Host,Reply};
use ovg_processing::photogrammetry::{images,exif};
use serde::Serialize;
use std::{collections::BTreeSet,fs::{self,File},io::Read,path::PathBuf};
use tauri::{State,WebviewWindow};
use tauri_plugin_dialog::DialogExt;
use uuid::Uuid;

#[derive(Debug,Serialize)]
pub struct Inventory{
 pub count:u32,pub bytes:u64,pub pixels:u64,pub gps_count:u32,pub metadata_errors:u32,
 pub non_default_orientation:u32,pub min_width:u32,pub max_width:u32,pub min_height:u32,pub max_height:u32,
}
fn inspect(paths:&[PathBuf])->Result<Inventory,&'static str>{
 if paths.len()<2||paths.len()>4096{return Err("PHOTO_COUNT_LIMIT");}
 let mut out=Inventory{count:0,bytes:0,pixels:0,gps_count:0,metadata_errors:0,non_default_orientation:0,
 min_width:u32::MAX,max_width:0,min_height:u32::MAX,max_height:0};
 let limits=images::Limits{max_header_bytes:1024*1024,max_side:32768,max_pixels:80_000_000};
 let mut seen=BTreeSet::new();
 for p in paths{
 let m=fs::symlink_metadata(p).map_err(|_|"INVALID_PHOTO")?;
 if !m.is_file()||m.file_type().is_symlink()||m.len()==0||m.len()>256*1024*1024{return Err("INVALID_PHOTO");}
 let p=p.canonicalize().map_err(|_|"INVALID_PHOTO")?;
 if !seen.insert(p.clone()){return Err("DUPLICATE_PHOTO");}
 out.bytes=out.bytes.checked_add(m.len()).ok_or("RESOURCE_LIMIT")?;
 if out.bytes>1u64<<40{return Err("RESOURCE_LIMIT");}
 let mut data=Vec::new();File::open(&p).map_err(|_|"INVALID_PHOTO")?
 .take(limits.max_header_bytes as u64).read_to_end(&mut data).map_err(|_|"INVALID_PHOTO")?;
 let d=images::jpeg_dimensions(&data,limits).map_err(|_|"INVALID_PHOTO_HEADER")?;
 out.pixels=out.pixels.checked_add(u64::from(d.width)*u64::from(d.height)).ok_or("RESOURCE_LIMIT")?;
 out.count+=1;out.min_width=out.min_width.min(d.width);out.max_width=out.max_width.max(d.width);
 out.min_height=out.min_height.min(d.height);out.max_height=out.max_height.max(d.height);
 match exif::jpeg(&data,limits.max_header_bytes){
 Ok(Some(m))=>{if m.gps.is_some(){out.gps_count+=1;}if m.orientation.is_some_and(|v|v!=1){out.non_default_orientation+=1;}}
 Ok(None)=>{},Err(_)=>out.metadata_errors+=1,
 }
 }
 Ok(out)
}
#[tauri::command]
pub async fn desktop_photo_inspect(window:WebviewWindow,host:State<'_,Host>,handle:Uuid)->Reply<Option<Inventory>>{
 let app=window.app_handle().clone();
 crate::run(window,host.inner().clone(),move|h|{
 h.generation(handle)?;
 let Some(files)=app.dialog().file().add_filter("JPEG flight images",&["jpg","jpeg","JPG","JPEG"]).blocking_pick_files() else{return Ok(None);};
 if files.len()>4096{return Err("PHOTO_COUNT_LIMIT");}
 let paths=files.into_iter().map(|f|f.into_path().map_err(|_|"INVALID_PHOTO")).collect::<Result<Vec<_>,_>>()?;
 let inventory=inspect(&paths)?;h.generation(handle)?;Ok(Some(inventory))
 }).await
}
#[cfg(test)]mod tests{
 use super::*;
 #[test]fn inventory_and_duplicate(){
 let t=tempfile::tempdir().unwrap();let a=t.path().join("a.jpg");let b=t.path().join("b.jpg");
 let bytes=[0xffu8,0xd8,0xff,0xc0,0,11,8,0,7,0,9,1,1,0x11,0,0xff,0xda];
 fs::write(&a,bytes).unwrap();fs::write(&b,bytes).unwrap();
 let x=inspect(&[a.clone(),b]).unwrap();assert_eq!(x.count,2);assert_eq!(x.pixels,126);
 assert!(matches!(inspect(&[a.clone(),a]),Err("DUPLICATE_PHOTO")));
 }
}

use std::sync::{Arc,Mutex,OnceLock,atomic::{AtomicBool,AtomicU64,Ordering}};
use ovg_processing::photogrammetry::ingest;
const MAX_DATASET:u64=8*1024*1024*1024;
#[derive(Debug,Clone,serde::Deserialize,Serialize)]
#[serde(deny_unknown_fields)]
pub struct Dataset{pub count:u32,pub bytes:u64,pub manifest_sha256:String}
#[derive(Serialize)]pub struct Progress{pub completed:u64,pub copied:u64,pub total_files:u64,pub total_bytes:u64}
struct Active{handle:Uuid,id:Uuid,cancel:AtomicBool,completed:AtomicU64,copied:AtomicU64,total_files:AtomicU64,total_bytes:AtomicU64}
static ACTIVE:OnceLock<Mutex<Option<Arc<Active>>>>=OnceLock::new();
fn active()->&'static Mutex<Option<Arc<Active>>>{ACTIVE.get_or_init(||Mutex::new(None))}
struct Guard(Arc<Active>);
impl Drop for Guard{fn drop(&mut self){if let Ok(mut s)=active().lock(){if s.as_ref().is_some_and(|v|Arc::ptr_eq(v,&self.0)){*s=None;}}}}
fn acquire(handle:Uuid,id:Uuid)->Result<Guard,&'static str>{
 let mut s=active().lock().map_err(|_|"SESSION_FAILED")?;
 if s.is_some(){return Err("PHOTO_JOB_BUSY");}
 let job=Arc::new(Active{handle,id,cancel:AtomicBool::new(false),completed:AtomicU64::new(0),copied:AtomicU64::new(0),total_files:AtomicU64::new(0),total_bytes:AtomicU64::new(0)});
 *s=Some(job.clone());Ok(Guard(job))
}
fn read_dataset(project:&std::path::Path)->Result<Option<Dataset>,&'static str>{
 let dir=project.join("photogrammetry-input");
 match fs::symlink_metadata(&dir){Err(e)if e.kind()==std::io::ErrorKind::NotFound=>return Ok(None),
 Ok(m)if m.file_type().is_dir()=>{},_=>return Err("INVALID_PHOTO_DATASET")}
 let p=dir.join("dataset.json");let m=fs::symlink_metadata(&p).map_err(|_|"INVALID_PHOTO_DATASET")?;
 if !m.file_type().is_file()||m.len()>4096{return Err("INVALID_PHOTO_DATASET");}
 let mut b=Vec::new();File::open(p).map_err(|_|"INVALID_PHOTO_DATASET")?.take(4097).read_to_end(&mut b).map_err(|_|"INVALID_PHOTO_DATASET")?;
 let d:Dataset=serde_json::from_slice(&b).map_err(|_|"INVALID_PHOTO_DATASET")?;
 if d.count<2||d.count>4096||d.bytes==0||d.bytes>MAX_DATASET||d.manifest_sha256.len()!=64||
 !d.manifest_sha256.bytes().all(|b|b.is_ascii_hexdigit()&&!b.is_ascii_uppercase()){return Err("INVALID_PHOTO_DATASET");}
 Ok(Some(d))
}
#[tauri::command]
pub async fn desktop_photo_dataset(window:WebviewWindow,host:State<'_,Host>,handle:Uuid)->Reply<Option<Dataset>>{
 crate::run(window,host.inner().clone(),move|h|read_dataset(&h.photo_project_dir(handle)?)).await
}
#[tauri::command]
pub fn desktop_photo_status(window:WebviewWindow,host:State<'_,Host>,handle:Uuid,id:Uuid)->Reply<Option<Progress>>{
 crate::trusted(&window)?;host.generation(handle).map_err(str::to_owned)?;
 let s=active().lock().map_err(|_|"SESSION_FAILED".to_owned())?;
 Ok(s.as_ref().filter(|v|v.handle==handle&&v.id==id).map(|v|Progress{completed:v.completed.load(Ordering::Acquire),copied:v.copied.load(Ordering::Acquire),total_files:v.total_files.load(Ordering::Acquire),total_bytes:v.total_bytes.load(Ordering::Acquire)}))
}
#[tauri::command]
pub fn desktop_photo_cancel(window:WebviewWindow,host:State<'_,Host>,handle:Uuid,id:Uuid)->Reply<()>{
 crate::trusted(&window)?;host.generation(handle).map_err(str::to_owned)?;
 let s=active().lock().map_err(|_|"SESSION_FAILED".to_owned())?;
 if let Some(v)=s.as_ref().filter(|v|v.handle==handle&&v.id==id){v.cancel.store(true,Ordering::Release);}
 Ok(())
}
#[tauri::command]
pub async fn desktop_photo_stage(window:WebviewWindow,host:State<'_,Host>,handle:Uuid,id:Uuid)->Reply<Option<Dataset>>{
 let app=window.app_handle().clone();
 crate::run(window,host.inner().clone(),move|h|{
 let project=h.photo_project_dir(handle)?;
 let guard=acquire(handle,id)?;let job=&guard.0;
 if read_dataset(&project)?.is_some(){return Err("PHOTO_DATASET_EXISTS");}
 let Some(files)=app.dialog().file().add_filter("JPEG flight images",&["jpg","jpeg","JPG","JPEG"]).blocking_pick_files() else{return Ok(None);};
 if files.len()>4096{return Err("PHOTO_COUNT_LIMIT");}
 let paths=files.into_iter().map(|f|f.into_path().map_err(|_|"INVALID_PHOTO")).collect::<Result<Vec<_>,_>>()?;
 let inventory=inspect(&paths)?;if inventory.bytes>MAX_DATASET{return Err("PHOTO_DATASET_LIMIT");}
 job.total_files.store(u64::from(inventory.count),Ordering::Release);
 job.total_bytes.store(inventory.bytes,Ordering::Release);
 let temp=tempfile::Builder::new().prefix(".photo-stage-").tempdir_in(&project).map_err(|_|"STORAGE_UNAVAILABLE")?;
 let stage=temp.path().join("input");
 let cancelled=||job.cancel.load(Ordering::Acquire)||h.generation(handle).is_err();
 let p=ingest::Policy{max_files:4096,max_file_bytes:256*1024*1024,max_total_bytes:MAX_DATASET,
 header:images::Limits{max_header_bytes:1024*1024,max_side:32768,max_pixels:80_000_000}};
 let entries=ingest::stage_jpeg_controlled(&stage,&paths,p,cancelled,|n,b|{job.completed.store(n as u64,Ordering::Release);job.copied.store(b,Ordering::Release);})
 .map_err(|e|if matches!(e,ingest::Error::Cancelled){"CANCELLED"}else{"PHOTO_STAGE_FAILED"})?;
 let bytes=entries.iter().try_fold(0u64,|n,e|n.checked_add(e.bytes)).ok_or("RESOURCE_LIMIT")?;
 let hash=ingest::verify_staged(&stage,entries.len() as u32,MAX_DATASET,cancelled).map_err(|_|if cancelled(){"CANCELLED"}else{"PHOTO_VERIFY_FAILED"})?;
 let d=Dataset{count:entries.len() as u32,bytes,manifest_sha256:hash};
 use std::io::Write;
 let mut f=File::options().write(true).create_new(true).open(stage.join("dataset.json")).map_err(|_|"STORAGE_UNAVAILABLE")?;
 f.write_all(&serde_json::to_vec(&d).map_err(|_|"INVALID_PHOTO_DATASET")?).map_err(|_|"STORAGE_UNAVAILABLE")?;
 f.sync_all().map_err(|_|"STORAGE_UNAVAILABLE")?;drop(f);
 if cancelled(){return Err("CANCELLED");}
 // One dataset per project in this candidate. Never replace an existing dataset.
 let dest=project.join("photogrammetry-input");if fs::symlink_metadata(&dest).is_ok(){return Err("PHOTO_DATASET_EXISTS");}
 fs::rename(&stage,&dest).map_err(|_|"STORAGE_UNAVAILABLE")?;
 Ok(Some(d))
 }).await
}
