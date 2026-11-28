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
