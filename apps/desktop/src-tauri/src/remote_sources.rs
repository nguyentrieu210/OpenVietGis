//! Explicit remote source operations. No automatic network access or saved URL.
use super::*;
use unified::{Unified,work};
#[tauri::command]
pub fn remote_range_status(window:WebviewWindow)->Reply<bool>{trusted(&window)?;Ok(cfg!(feature="remote-range"))}
#[tauri::command]
pub async fn remote_tiff_inspect(window:WebviewWindow,u:State<'_,Unified>,handle:Uuid,url:String,request_id:Uuid)->Reply<Value>{
 #[cfg(feature="remote-range")]{
  trusted(&window)?;let read=ReadPermit::acquire(request_id)?;
  let slot=tokio::select!{p=READ_SLOTS.get_or_init(||tokio::sync::Semaphore::new(4)).acquire()=>p.map_err(|_|"SESSION_FAILED".to_owned())?,_=read.token.notify.notified()=>return Err("CANCELLED".into())};
  work(window,u.inner().clone(),move|s|{
   let _slot=slot;
   s.with(handle,|_|Ok(()))?;
   let mut source=ovg_native_providers::http_range::RangeSource::open(&url,read.token.flag.clone()).map_err(|_|"REMOTE_OPEN_FAILED")?;
   let identity=source.identity().clone();
   if read.token.flag.load(Ordering::Acquire){return Err("CANCELLED")}
   let levels=ovg_native_providers::remote_tiff::TiledTiff::pyramid(&mut source,identity.bytes).map_err(|_|"REMOTE_TIFF_PROFILE_UNSUPPORTED")?;
   s.with(handle,|_|Ok(()))?;
   Ok(serde_json::json!({"identity":identity,"info":levels[0].info,"overviews":levels.iter().map(|d|&d.info).collect::<Vec<_>>() }))
  }).await
 }
 #[cfg(not(feature="remote-range"))]{let _=(u,handle,url,request_id);trusted(&window)?;Err("REMOTE_RANGE_NOT_LINKED".into())}
}
#[tauri::command]
pub async fn remote_tiff_tile(window:WebviewWindow,u:State<'_,Unified>,handle:Uuid,url:String,etag:String,bytes:u64,level:usize,col:u32,row:u32,request_id:Uuid)->Reply<tauri::ipc::Response>{
 #[cfg(feature="remote-range")]{
  trusted(&window)?;let read=ReadPermit::acquire(request_id)?;
  let slot=tokio::select!{
   p=READ_SLOTS.get_or_init(||tokio::sync::Semaphore::new(4)).acquire()=>p.map_err(|_|"SESSION_FAILED".to_owned())?,
   _=read.token.notify.notified()=>return Err("CANCELLED".into())
  };
  let result=work(window,u.inner().clone(),move|s|{
   let _slot=slot;
   s.with(handle,|_|Ok(()))?;
   if read.token.flag.load(Ordering::Acquire){return Err("CANCELLED")}
   let mut source=ovg_native_providers::http_range::RangeSource::open(&url,read.token.flag.clone()).map_err(|_|"REMOTE_OPEN_FAILED")?;
   source.require_identity(&ovg_native_providers::http_range::Identity{bytes,etag}).map_err(|_|"REMOTE_SOURCE_CHANGED")?;
   let levels=ovg_native_providers::remote_tiff::TiledTiff::pyramid(&mut source,bytes).map_err(|_|"REMOTE_TIFF_PROFILE_UNSUPPORTED")?;
   let t=levels.get(level).ok_or("INVALID_OVERVIEW")?;
   if read.token.flag.load(Ordering::Acquire){return Err("CANCELLED")}
   let b=t.tile(&mut source,col,row).map_err(|_|"REMOTE_TILE_FAILED")?;
   if read.token.flag.load(Ordering::Acquire){return Err("CANCELLED")}
   s.with(handle,|_|Ok(()))?;
   Ok(b)
  }).await?;
  Ok(tauri::ipc::Response::new(result))
 }
 #[cfg(not(feature="remote-range"))]{let _=(u,handle,url,etag,bytes,level,col,row,request_id);trusted(&window)?;Err("REMOTE_RANGE_NOT_LINKED".into())}
}

#[tauri::command]
pub fn remote_copc_status(window:WebviewWindow)->Reply<bool>{trusted(&window)?;Ok(cfg!(feature="remote-copc"))}
#[tauri::command]
pub async fn remote_copc_inspect(window:WebviewWindow,u:State<'_,Unified>,handle:Uuid,url:String,request_id:Uuid)->Reply<Value>{
 #[cfg(feature="remote-copc")]{
  trusted(&window)?;let read=ReadPermit::acquire(request_id)?;
  let slot=tokio::select!{p=READ_SLOTS.get_or_init(||tokio::sync::Semaphore::new(4)).acquire()=>p.map_err(|_|"SESSION_FAILED".to_owned())?,_=read.token.notify.notified()=>return Err("CANCELLED".into())};
  work(window,u.inner().clone(),move|s|{
   let _slot=slot;s.with(handle,|_|Ok(()))?;
   let mut source=ovg_native_providers::http_range::RangeSource::open(&url,read.token.flag.clone()).map_err(|_|"REMOTE_OPEN_FAILED")?;
   let identity=source.identity().clone();let c=ovg_native_providers::remote_copc::Copc::open(&mut source,identity.bytes).map_err(|_|"REMOTE_COPC_PROFILE_UNSUPPORTED")?;
   s.with(handle,|_|Ok(()))?;
   Ok(serde_json::json!({"identity":identity,"info":c.info}))
  }).await
 }
 #[cfg(not(feature="remote-copc"))]{let _=(u,handle,url,request_id);trusted(&window)?;Err("REMOTE_COPC_NOT_LINKED".into())}
}
#[tauri::command]
pub async fn remote_copc_query(window:WebviewWindow,u:State<'_,Unified>,handle:Uuid,url:String,etag:String,bytes:u64,bounds:[f64;6],level:i32,request_id:Uuid)->Reply<tauri::ipc::Response>{
 #[cfg(feature="remote-copc")]{
  trusted(&window)?;let read=ReadPermit::acquire(request_id)?;
  let slot=tokio::select!{p=READ_SLOTS.get_or_init(||tokio::sync::Semaphore::new(4)).acquire()=>p.map_err(|_|"SESSION_FAILED".to_owned())?,_=read.token.notify.notified()=>return Err("CANCELLED".into())};
  let result=work(window,u.inner().clone(),move|s|{
   let _slot=slot;s.with(handle,|_|Ok(()))?;
   let mut source=ovg_native_providers::http_range::RangeSource::open(&url,read.token.flag.clone()).map_err(|_|"REMOTE_OPEN_FAILED")?;
   source.require_identity(&ovg_native_providers::http_range::Identity{bytes,etag}).map_err(|_|"REMOTE_SOURCE_CHANGED")?;
   let c=ovg_native_providers::remote_copc::Copc::open(&mut source,bytes).map_err(|_|"REMOTE_COPC_PROFILE_UNSUPPORTED")?;
   let b=c.query(&mut source,bounds,level,&read.token.flag).map_err(|_|"REMOTE_COPC_QUERY_FAILED_OR_DENSE")?;
   s.with(handle,|_|Ok(()))?;Ok(b)
  }).await?;
  Ok(tauri::ipc::Response::new(result))
 }
 #[cfg(not(feature="remote-copc"))]{let _=(u,handle,url,etag,bytes,bounds,level,request_id);trusted(&window)?;Err("REMOTE_COPC_NOT_LINKED".into())}
}
