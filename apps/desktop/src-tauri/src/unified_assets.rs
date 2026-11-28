//! Shared session resource adapters in the unified workbench. No persistent source paths.
use super::*;
use unified::{Unified,work};
use serde::Serialize;
use std::path::Path;
type R<T>=Result<T,&'static str>;
pub enum Asset{
 Cad(crate::resource_files::Cad),Las(crate::resource_files::Las),Cloud(crate::compressed_points::Cloud),
 Glb(crate::resource_files::Glb),Raster(ovg_local_storage::mbtiles::Archive,tempfile::NamedTempFile),
 #[cfg(feature="native-ogr")] Tiff(ovg_native_providers::ogr::RasterSource)
}
#[derive(Serialize,Clone)]
pub struct Info{pub id:Uuid,pub kind:String,pub count:u64,pub epsg:Option<u32>,pub metadata:Value}
pub struct Entry{pub info:Info,pub asset:Asset}
impl Entry{
 pub fn open(path:&Path,kind:&str,epsg:Option<u32>)->R<Self>{
  let asset=match kind{
   "cad" if epsg==Some(4326)=>Asset::Cad(crate::resource_files::Cad::open(path)?),
   "las" if epsg.is_some_and(|n|n>0)=>Asset::Las(crate::resource_files::Las::open(path)?),
   "laz"|"copc" if epsg.is_some_and(|n|n>0)=>Asset::Cloud(crate::compressed_points::Cloud::open(path,kind=="copc")?),
   "glb" if epsg.is_none()=>Asset::Glb(crate::resource_files::Glb::open(path)?),
   "mbtiles" if epsg.is_none()=>{
    let staged=crate::gpkg_import::sqlite_snapshot(path,512*1024*1024)?;
    let archive=ovg_local_storage::mbtiles::Archive::open(staged.path()).map_err(|_|"INVALID_MBTILES")?;
    Asset::Raster(archive,staged)
   }
   #[cfg(feature="native-ogr")]
   "tiff" if epsg.is_none()=>Asset::Tiff(ovg_native_providers::ogr::RasterSource::open(path)?),
   _=>return Err("UNSUPPORTED_FORMAT_OR_CRS")
  };
  let (count,metadata,epsg)=match &asset{
   Asset::Cad(a)=>(a.count(),Value::Null,epsg),Asset::Las(a)=>(a.count(),Value::Null,epsg),
   Asset::Cloud(a)=>(a.count(),Value::Null,epsg),Asset::Glb(a)=>(a.len(),Value::Null,None),
   Asset::Raster(..)=>(0,Value::Null,Some(3857)),
   #[cfg(feature="native-ogr")]
   Asset::Tiff(a)=>(0,serde_json::to_value(&a.info).map_err(|_|"INVALID_RASTER")?,a.info.epsg)
  };
  Ok(Self{info:Info{id:Uuid::new_v4(),kind:kind.into(),count,epsg,metadata},asset})
 }
}
#[tauri::command]
pub async fn unified_asset_open(window:WebviewWindow,u:State<'_,Unified>,handle:Uuid,kind:String,epsg:Option<u32>)->Reply<Option<Info>>{
 let app=window.app_handle().clone();
 work(window,u.inner().clone(),move|s|{
  s.with(handle,|_|Ok(()))?;
  let (name,ext):(&str,&[&str])=match kind.as_str(){
   "cad"=>("DXF WGS84 XY",&["dxf"]),"las"=>("LAS",&["las"]),"laz"|"copc"=>("LAZ / COPC",&["laz"]),
   "glb"=>("Self-contained GLB",&["glb"]),"mbtiles"=>("Raster MBTiles",&["mbtiles"]),
   #[cfg(feature="native-ogr")] "tiff"=>("GeoTIFF",&["tif","tiff"]),
   _=>return Err("UNSUPPORTED_FORMAT")
  };
  let Some(file)=app.dialog().file().add_filter(name,ext).blocking_pick_file() else{return Ok(None)};
  let path=file.into_path().map_err(|_|"INVALID_STORAGE")?;
  s.open_asset(handle,&path,&kind,epsg).map(Some)
 }).await
}
#[tauri::command]
pub async fn unified_asset_close(window:WebviewWindow,u:State<'_,Unified>,handle:Uuid,id:Uuid)->Reply<()>{
 work(window,u.inner().clone(),move|s|s.close_asset(handle,id)).await
}
#[tauri::command]
pub async fn unified_asset_read(window:WebviewWindow,u:State<'_,Unified>,handle:Uuid,id:Uuid,chunk:u32,bbox:Option<[f64;4]>,bounds:Option<[f64;6]>,level:Option<i32>,tile:Option<[u32;3]>,band:Option<usize>,pixel:Option<[usize;4]>,shape:Option<[usize;2]>)->Reply<tauri::ipc::Response>{
 let b=work(window,u.inner().clone(),move|s|s.with_asset(handle,id,|e|{
  let b=match &mut e.asset{
   Asset::Cad(a)=>a.viewport(bbox.ok_or("BBOX_REQUIRED")?)?,
   Asset::Las(a)=>a.chunk(chunk)?,
   Asset::Cloud(a)=>if let Some(b)=bounds{a.query(b,level.ok_or("LOD_REQUIRED")?)?}else{a.chunk(chunk)?},
   Asset::Glb(a)=>a.bytes(),
   Asset::Raster(a,_)=>{
    let [z,x,y]=tile.ok_or("TILE_REQUIRED")?;
    if z>22||x>=(1u32<<z)||y>=(1u32<<z){return Err("INVALID_TILE")}
    let b=a.tile(z as u8,x,y).map_err(|_|"INVALID_TILE")?.ok_or("TILE_NOT_FOUND")?;
    if b.len()<33||b.len()>4*1024*1024||b[..8]!=[137,80,78,71,13,10,26,10]||&b[12..16]!=b"IHDR"{return Err("INVALID_TILE")}
    let w=u32::from_be_bytes(b[16..20].try_into().map_err(|_|"INVALID_TILE")?);let h=u32::from_be_bytes(b[20..24].try_into().map_err(|_|"INVALID_TILE")?);
    if !matches!(w,256|512)||h!=w{return Err("INVALID_TILE")}b
   },
   #[cfg(feature="native-ogr")]
   Asset::Tiff(a)=>a.window(band.ok_or("BAND_REQUIRED")?,pixel.ok_or("WINDOW_REQUIRED")?,shape.ok_or("SHAPE_REQUIRED")?)?
  };
  if b.len()>4*1024*1024{return Err("RESOURCE_LIMIT")}Ok(b)
 })).await?;
 Ok(tauri::ipc::Response::new(b))
}

#[cfg(test)]
mod tests{
 use super::*;
 #[test]fn cad_grant_scope_and_close(){
  let d=tempfile::tempdir().unwrap();let path=d.path().join("point.dxf");
  std::fs::write(&path,"0\nSECTION\n2\nENTITIES\n0\nPOINT\n5\nAB\n8\nSurvey\n10\n106\n20\n11\n0\nENDSEC\n0\nEOF\n").unwrap();
  let u=Unified::new(d.path().join("projects")).unwrap();
  let a=u.open(u.create().unwrap().id).unwrap();let b=u.open(u.create().unwrap().id).unwrap();
  let info=u.open_asset(a.handle,&path,"cad",Some(4326)).unwrap();assert_eq!(info.count,1);
  assert!(u.with_asset(b.handle,info.id,|_|Ok(())).is_err());
  assert!(u.with_asset(a.handle,info.id,|e|match &e.asset{Asset::Cad(c)=>c.viewport([105.,10.,107.,12.]),_=>Err("INVALID_ASSET")}).is_ok());
  u.close(a.handle).unwrap();assert!(u.with_asset(a.handle,info.id,|_|Ok(())).is_err());u.shutdown().unwrap();
 }
}

pub fn filename(kind:&str)->R<&'static str>{Ok(match kind{
 "cad"=>"source.dxf","las"=>"source.las","laz"|"copc"=>"source.laz","glb"=>"source.glb","mbtiles"=>"source.mbtiles","tiff"=>"source.tif",_=>return Err("UNSUPPORTED_FORMAT")
})}
#[tauri::command]
pub async fn unified_asset_attach(window:WebviewWindow,u:State<'_,Unified>,handle:Uuid,kind:String,epsg:Option<u32>,expected:String)->Reply<Option<ovg_project_store::asset_store::AssetMeta>>{
 let app=window.app_handle().clone();
 work(window,u.inner().clone(),move|s|{
  s.with(handle,|_|Ok(()))?;
  let ext=filename(&kind)?.split('.').last().ok_or("UNSUPPORTED_FORMAT")?;
  let filters=if kind=="tiff"{vec!["tif","tiff"]}else{vec![ext]};
  let Some(file)=app.dialog().file().add_filter("Project asset (copy)",&filters).blocking_pick_file() else{return Ok(None)};
  let path=file.into_path().map_err(|_|"INVALID_STORAGE")?;
  s.attach_asset(handle,&path,&kind,epsg,unified::number(&expected)?).map(Some)
 }).await
}
#[tauri::command]
pub async fn unified_asset_list(window:WebviewWindow,u:State<'_,Unified>,handle:Uuid)->Reply<Vec<ovg_project_store::asset_store::AssetMeta>>{
 work(window,u.inner().clone(),move|s|s.with(handle,|store|store.assets().map_err(|_|"STORE_QUERY_FAILED"))).await
}
#[tauri::command]
pub async fn unified_asset_reopen(window:WebviewWindow,u:State<'_,Unified>,handle:Uuid,id:Uuid)->Reply<Info>{
 work(window,u.inner().clone(),move|s|s.reopen_asset(handle,id)).await
}
#[tauri::command]
pub async fn unified_asset_detach(window:WebviewWindow,u:State<'_,Unified>,handle:Uuid,id:Uuid,expected:String)->Reply<String>{
 work(window,u.inner().clone(),move|s|Ok(s.detach_asset(handle,id,unified::number(&expected)?)?.to_string())).await
}
#[tauri::command]
pub async fn unified_import_ndjson(window:WebviewWindow,u:State<'_,Unified>,handle:Uuid,name:String,expected:String)->Reply<Option<Value>>{
 let app=window.app_handle().clone();
 work(window,u.inner().clone(),move|s|{
  s.with(handle,|_|Ok(()))?;
  let Some(file)=app.dialog().file().add_filter("NDJSON WGS84 features",&["ndjson","geojsonl","jsonl"]).blocking_pick_file() else{return Ok(None)};
  let path=file.into_path().map_err(|_|"INVALID_STORAGE")?;
  let r=s.import_ndjson(handle,&path,&name,unified::number(&expected)?)?;
  Ok(Some(serde_json::json!({"layer":r.layer,"revision":r.revision.to_string(),"count":r.count,"digest":r.digest})))
 }).await
}
