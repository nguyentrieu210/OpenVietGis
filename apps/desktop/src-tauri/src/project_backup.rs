//! Full standalone project bundle: bounded, no arbitrary archive extraction.
use crate::host::LayerInfo;
use serde::{Serialize,Deserialize};
use std::{collections::HashSet,fs,io::{Read,Write},path::Path};
use uuid::Uuid;
use zip::{ZipArchive,ZipWriter,write::SimpleFileOptions,CompressionMethod};
type Result<T>=std::result::Result<T,&'static str>;
const MAX_FILE:u64=512*1024*1024;
const MAX_TOTAL:u64=2*1024*1024*1024;
const MANIFEST_MAX:u64=96*1024;
#[derive(Serialize,Deserialize)]
#[serde(deny_unknown_fields)]
struct Manifest{version:u32,layers:Vec<LayerInfo>,#[serde(default)]state:Option<crate::project_state::ProjectState>}
fn path_for(id:Uuid)->String{format!("layers/{id}/workspace.gpkg")}
pub fn write_bundle(stage:&Path,layers:&[LayerInfo],state:&crate::project_state::ProjectState,dest:&Path)->Result<()>{
 if layers.len()>32{return Err("RESOURCE_LIMIT")}
 let manifest=serde_json::to_vec(&Manifest{version:1,layers:layers.to_vec(),state:Some(state.clone())}).map_err(|_|"INVALID_PROJECT")?;
 if manifest.len()as u64>MANIFEST_MAX{return Err("RESOURCE_LIMIT")}
 let f=fs::OpenOptions::new().write(true).create_new(true).open(dest).map_err(|_|"DESTINATION_UNAVAILABLE")?;
 let result=(||{
   let mut zip=ZipWriter::new(f);let opts=SimpleFileOptions::default().compression_method(CompressionMethod::Stored);
   zip.start_file("manifest.json",opts).map_err(|_|"STORAGE_UNAVAILABLE")?;
   zip.write_all(&manifest).map_err(|_|"STORAGE_UNAVAILABLE")?;
   let mut total=manifest.len()as u64;
   for name in std::iter::once("workspace.gpkg".to_owned()).chain(layers.iter().map(|l|path_for(l.id))){
      let p=stage.join(&name);
      let mut src=fs::File::open(&p).map_err(|_|"STORAGE_UNAVAILABLE")?;
      let size=src.metadata().map_err(|_|"STORAGE_UNAVAILABLE")?.len();
      total=total.checked_add(size).ok_or("RESOURCE_LIMIT")?;
      if size>MAX_FILE||total>MAX_TOTAL{return Err("RESOURCE_LIMIT")}
      zip.start_file(&name,opts).map_err(|_|"STORAGE_UNAVAILABLE")?;
      let copied=std::io::copy(&mut src.take(MAX_FILE+1),&mut zip).map_err(|_|"STORAGE_UNAVAILABLE")?;
      if copied!=size{return Err("SOURCE_CHANGED")}
   }
   zip.finish().map_err(|_|"STORAGE_UNAVAILABLE")?.sync_all().map_err(|_|"STORAGE_UNAVAILABLE")?;
   Ok(())
 })();
 if result.is_err(){let _=fs::remove_file(dest);}
 result
}
pub fn restore_bundle(source:&Path,stage:&Path)->Result<()>{
 let meta=fs::symlink_metadata(source).map_err(|_|"INVALID_PROJECT")?;
 if !meta.file_type().is_file()||meta.len()>MAX_TOTAL+MANIFEST_MAX{return Err("RESOURCE_LIMIT")}
 let mut zip=ZipArchive::new(fs::File::open(source).map_err(|_|"INVALID_PROJECT")?).map_err(|_|"INVALID_PROJECT")?;
 if zip.len()<2||zip.len()>34{return Err("INVALID_PROJECT")}
 let manifest:Manifest={
   let file=zip.by_name("manifest.json").map_err(|_|"INVALID_PROJECT")?;
   if file.size()>MANIFEST_MAX||file.compression()!=CompressionMethod::Stored{return Err("INVALID_PROJECT")}
   let mut b=Vec::new();file.take(MANIFEST_MAX+1).read_to_end(&mut b).map_err(|_|"INVALID_PROJECT")?;
   if b.len()as u64>MANIFEST_MAX{return Err("RESOURCE_LIMIT")}
   serde_json::from_slice(&b).map_err(|_|"INVALID_PROJECT")?
 };
 if manifest.version!=1||manifest.layers.len()>32||zip.len()!=manifest.layers.len()+2{return Err("INVALID_PROJECT")}
 let mut allowed=HashSet::from(["manifest.json".to_owned(),"workspace.gpkg".to_owned()]);
 for l in &manifest.layers{
   if l.id.is_nil()||l.name.trim().is_empty()||l.name.len()>256||!allowed.insert(path_for(l.id)){return Err("INVALID_LAYER")}
 }
 let mut seen=HashSet::new();let mut total=0u64;
 for i in 0..zip.len(){
   let f=zip.by_index(i).map_err(|_|"INVALID_PROJECT")?;
   if f.is_dir()||f.compression()!=CompressionMethod::Stored||!allowed.contains(f.name())||!seen.insert(f.name().to_owned()){return Err("INVALID_PROJECT")}
   let size=f.size();
   total=total.checked_add(size).ok_or("RESOURCE_LIMIT")?;
   if size>MAX_FILE||total>MAX_TOTAL{return Err("RESOURCE_LIMIT")}
 }
 if seen!=allowed{return Err("INVALID_PROJECT")}
 let input=tempfile::tempdir_in(stage).map_err(|_|"STORAGE_UNAVAILABLE")?;
 for (id,name) in std::iter::once((None,"workspace.gpkg".to_owned())).chain(manifest.layers.iter().map(|l|(Some(l.id),path_for(l.id)))){
   let p=input.path().join(format!("{}.gpkg",id.map(|i|i.to_string()).unwrap_or_else(||"root".into())));
   let mut dst=fs::OpenOptions::new().write(true).create_new(true).open(&p).map_err(|_|"STORAGE_UNAVAILABLE")?;
   let f=zip.by_name(&name).map_err(|_|"INVALID_PROJECT")?;let size=f.size();
   let n=std::io::copy(&mut f.take(MAX_FILE+1),&mut dst).map_err(|_|"INVALID_PROJECT")?;
   if n!=size{return Err("INVALID_PROJECT")}
   dst.sync_all().map_err(|_|"STORAGE_UNAVAILABLE")?;drop(dst);
   let dir=match id{None=>stage.to_path_buf(),Some(i)=>stage.join("layers").join(i.to_string())};
   fs::create_dir_all(&dir).map_err(|_|"STORAGE_UNAVAILABLE")?;
   crate::snapshot_restore::restore(&p,&dir)?;
   if let Some(i)=id{
     let layer=manifest.layers.iter().find(|l|l.id==i).ok_or("INVALID_LAYER")?;
     let mut f=fs::OpenOptions::new().write(true).create_new(true).open(dir.join("layer.json")).map_err(|_|"STORAGE_UNAVAILABLE")?;
     serde_json::to_writer(&mut f,layer).map_err(|_|"INVALID_LAYER")?;
     f.sync_all().map_err(|_|"STORAGE_UNAVAILABLE")?;
   }
 }
 if let Some(state)=manifest.state{let ids:Vec<_>=manifest.layers.iter().map(|l|l.id).collect();state.validate(&crate::project_state::known(&ids))?;crate::project_state::write_exact(stage,&state)?;}
 Ok(())
}
