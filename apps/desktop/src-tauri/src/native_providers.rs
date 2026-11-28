//! Native provider commands. Default build fails closed when drivers are absent.
use super::*;
use unified::{Unified,work};
#[tauri::command]
pub fn native_provider_status(window:WebviewWindow)->Reply<Value>{
 trusted(&window)?;
 Ok(serde_json::json!({"ogr_linked":cfg!(feature="native-ogr"),"analytics_linked":cfg!(feature="native-analytics"),"qualification":"candidate","spatial":"requires signed locally installed extension","parquet":"requires GDAL Parquet driver"}))
}
#[tauri::command]
pub async fn native_import_ogr(window:WebviewWindow,u:State<'_,Unified>,handle:Uuid,layer:Uuid,index:usize,expected:String)->Reply<Option<Value>>{
 #[cfg(feature="native-ogr")]{
  if index>=64{return Err("INVALID_LAYER".into())}
  let app=window.app_handle().clone();
  work(window,u.inner().clone(),move|s|{
   s.with(handle,|_|Ok(()))?;
   let Some(file)=app.dialog().file().add_filter("Shapefile / FlatGeobuf / GeoParquet",&["shp","fgb","parquet"]).blocking_pick_file() else{return Ok(None)};
   let path=file.into_path().map_err(|_|"INVALID_STORAGE")?;
   s.import_ogr(handle,layer,&path,index,expected).map(Some)
  }).await
 }
 #[cfg(not(feature="native-ogr"))]{let _=(u,handle,layer,index,expected);trusted(&window)?;Err("NATIVE_OGR_NOT_LINKED".into())}
}
#[tauri::command]
pub async fn native_proj(window:WebviewWindow,u:State<'_,Unified>,handle:Uuid,source:u32,target:u32,xy:Vec<[f64;2]>)->Reply<Vec<[f64;2]>>{
 #[cfg(feature="native-ogr")]{
  work(window,u.inner().clone(),move|s|s.with(handle,|_|ovg_native_providers::ogr::transform(source,target,&xy))).await
 }
 #[cfg(not(feature="native-ogr"))]{let _=(u,handle,source,target,xy);trusted(&window)?;Err("NATIVE_PROJ_NOT_LINKED".into())}
}
#[tauri::command]
pub async fn native_point_summary(window:WebviewWindow,u:State<'_,Unified>,handle:Uuid,layer:Uuid,expected:String,spatial:bool)->Reply<Value>{
 #[cfg(feature="native-analytics")]{
  work(window,u.inner().clone(),move|s|s.with(handle,|store|{
   let rev=unified::number(&expected)?;
   let mut pts=Vec::new();let mut after=0;
   loop{
    let p=store.query(layer,[-180.,-90.,180.,90.],after,200,rev).map_err(|_|"STORE_QUERY_FAILED")?;
    for r in p.rows{
     let g=ovg_spatial::parse_xy_wkb(&r.image.wkb,&ovg_spatial::ValidationLimits::default()).map_err(|_|"INVALID_GEOMETRY")?;
     let v=ovg_spatial::to_geojson(&g.geometry);
     if v["type"]!="Point"{return Err("POINT_LAYER_REQUIRED")}
     let x=v["coordinates"][0].as_f64().ok_or("INVALID_POINT")?;let y=v["coordinates"][1].as_f64().ok_or("INVALID_POINT")?;
     if pts.len()>=100_000{return Err("RESOURCE_LIMIT")}pts.push([x,y]);
    }
    match p.next{Some(n) if n>after=>after=n,Some(_)=>return Err("INVALID_CURSOR"),None=>break}
   }
   let report=ovg_native_providers::analytics::summarize(&pts,spatial)?;
   serde_json::to_value(report).map_err(|_|"INVALID_REPORT")
  })).await
 }
 #[cfg(not(feature="native-analytics"))]{let _=(u,handle,layer,expected,spatial);trusted(&window)?;Err("NATIVE_ANALYTICS_NOT_LINKED".into())}
}

#[tauri::command]
pub async fn native_raster_preview(window:WebviewWindow,u:State<'_,Unified>,handle:Uuid,band:usize)->Reply<Option<Value>>{
 #[cfg(feature="native-ogr")]{
  let app=window.app_handle().clone();
  work(window,u.inner().clone(),move|s|{
   s.with(handle,|_|Ok(()))?;
   let Some(file)=app.dialog().file().add_filter("Local GeoTIFF",&["tif","tiff"]).blocking_pick_file() else{return Ok(None)};
   let p=file.into_path().map_err(|_|"INVALID_STORAGE")?;
   let v=ovg_native_providers::ogr::raster_preview(&p,band)?;
   serde_json::to_value(v).map(Some).map_err(|_|"INVALID_RASTER")
  }).await
 }
 #[cfg(not(feature="native-ogr"))]{let _=(u,handle,band);trusted(&window)?;Err("NATIVE_OGR_NOT_LINKED".into())}
}

#[tauri::command]
pub async fn native_export_parquet(window:WebviewWindow,u:State<'_,Unified>,handle:Uuid,layer:Uuid,expected:String)->Reply<Option<u64>>{
 #[cfg(feature="native-ogr")]{
  let app=window.app_handle().clone();
  work(window,u.inner().clone(),move|s|{
   s.with(handle,|_|Ok(()))?;
   let Some(file)=app.dialog().file().add_filter("GeoParquet",&["parquet"]).set_file_name("layer.parquet").blocking_save_file() else{return Ok(None)};
   let p=file.into_path().map_err(|_|"INVALID_STORAGE")?;
   if p.extension().and_then(|x|x.to_str())!=Some("parquet"){return Err("UNSUPPORTED_FORMAT")}
   s.export_parquet(handle,layer,&p,unified::number(&expected)?).map(Some)
  }).await
 }
 #[cfg(not(feature="native-ogr"))]{let _=(u,handle,layer,expected);trusted(&window)?;Err("NATIVE_OGR_NOT_LINKED".into())}
}

#[tauri::command]
pub async fn native_import_ogr_stream(window:WebviewWindow,u:State<'_,Unified>,handle:Uuid,index:usize,name:String,expected:String,request_id:Uuid)->Reply<Option<Value>>{
 #[cfg(feature="native-ogr")]{
  trusted(&window)?;
  if index>=64||name.trim().is_empty()||name.len()>256{return Err("INVALID_LAYER".into())}
  let read=ReadPermit::acquire(request_id)?;
  let app=window.app_handle().clone();
  work(window,u.inner().clone(),move|s|{
   let Some(file)=app.dialog().file().add_filter("Stream SHP / FGB / GeoParquet",&["shp","fgb","parquet"]).blocking_pick_file() else{return Ok(None)};
   let path=file.into_path().map_err(|_|"INVALID_STORAGE")?;
   let r=s.import_ogr_stream(handle,&path,index,&name,unified::number(&expected)?,&read.token.flag)?;
   Ok(Some(serde_json::json!({"layer":r.layer,"revision":r.revision.to_string(),"count":r.count,"digest":r.digest})))
  }).await
 }
 #[cfg(not(feature="native-ogr"))]{let _=(u,handle,index,name,expected,request_id);trusted(&window)?;Err("NATIVE_OGR_NOT_LINKED".into())}
}
