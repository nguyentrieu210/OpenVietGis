#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
mod host;
mod mvt;
mod gpkg_import;
mod resource_files;
mod compressed_points;
mod snapshot_restore;
#[cfg(test)]
mod recovery_tests;
#[path="../../../../infra/desktop/wave0/protocol.rs"]
mod protocol;
use host::{Host, Opened, Poll, Project, Raster, AssetInfo, LayerInfo};
use serde::Serialize;
use serde_json::Value;
use tauri_plugin_dialog::DialogExt;
use tauri::{Emitter, Manager, State, WebviewWindow};
use uuid::Uuid;
use std::sync::{Arc,Mutex,OnceLock,atomic::{AtomicBool,AtomicUsize, Ordering}};
use std::collections::HashMap;
type Requests=HashMap<Uuid,Arc<AtomicBool>>;
static READS:OnceLock<Mutex<Requests>>=OnceLock::new();
struct ReadPermit{ id:Uuid, token:Arc<AtomicBool> }
impl ReadPermit{
    fn acquire(id:Uuid)->Reply<Self>{
        let mut r=READS.get_or_init(||Mutex::new(HashMap::new())).lock().map_err(|_|"SESSION_FAILED".to_owned())?;
        if r.len()>=4||r.contains_key(&id){return Err("RESOURCE_LIMIT".into())}
        let token=Arc::new(AtomicBool::new(false));r.insert(id,token.clone());Ok(Self{id,token})
    }
}
impl Drop for ReadPermit{fn drop(&mut self){if let Ok(mut r)=READS.get_or_init(||Mutex::new(HashMap::new())).lock(){r.remove(&self.id);}}}
#[tauri::command]
fn desktop_cancel_resource(window:WebviewWindow,request_id:Uuid)->Reply<()>{
    trusted(&window)?;
    let r=READS.get_or_init(||Mutex::new(HashMap::new())).lock().map_err(|_|"SESSION_FAILED".to_owned())?;
    if let Some(token)=r.get(&request_id){token.store(true,Ordering::Release);}
    Ok(())
}

static IN_FLIGHT: AtomicUsize = AtomicUsize::new(0);
struct Permit;
impl Permit {
    fn acquire()->Reply<Self>{
        IN_FLIGHT.fetch_update(Ordering::AcqRel,Ordering::Acquire,|n|if n<16 {Some(n+1)} else {None})
            .map(|_|Self).map_err(|_|"RESOURCE_LIMIT".to_owned())
    }
}
impl Drop for Permit {fn drop(&mut self){IN_FLIGHT.fetch_sub(1,Ordering::AcqRel);}}


type Reply<T> = Result<T,String>;
fn trusted(window:&WebviewWindow)->Reply<()>{
    let u=window.url().map_err(|_|"UNTRUSTED_WINDOW".to_owned())?;
    if window.label()!="main" || !matches!((u.scheme(),u.host_str()),("tauri",Some("localhost"))|("http"|"https",Some("tauri.localhost"))) {
        return Err("UNTRUSTED_WINDOW".into());
    }
    Ok(())
}
async fn run<T:Send+'static>(window:WebviewWindow,host:Host,f:impl FnOnce(Host)->Result<T,&'static str>+Send+'static)->Reply<T>{
    trusted(&window)?;
    let permit=Permit::acquire()?;
    tauri::async_runtime::spawn_blocking(move||{let _permit=permit;f(host)}).await.map_err(|_|"SESSION_FAILED".to_owned())?.map_err(str::to_owned)
}
#[derive(Serialize)]
struct Capabilities { protocol_version:u32, standalone:bool, managed:bool, cloud:bool, local_tiles:bool, transform:bool, local_edit:bool, viewport:bool, raster_archive:bool }
#[tauri::command]
fn desktop_capabilities(window:WebviewWindow)->Reply<Capabilities>{
    trusted(&window)?;
    Ok(Capabilities{protocol_version:1,standalone:true,managed:false,cloud:false,local_tiles:true,transform:true,local_edit:true,viewport:true,raster_archive:true})
}
#[tauri::command]
async fn desktop_list(window:WebviewWindow,host:State<'_,Host>)->Reply<Vec<Project>>{
    run(window,host.inner().clone(),|h|h.list()).await
}
#[tauri::command]
async fn desktop_create(window:WebviewWindow,host:State<'_,Host>)->Reply<Project>{
    run(window,host.inner().clone(),|h|h.create()).await
}
#[tauri::command]
async fn desktop_open(window:WebviewWindow,host:State<'_,Host>,id:Uuid)->Reply<Opened>{
    run(window,host.inner().clone(),move|h|h.open(id)).await
}
#[tauri::command]
async fn desktop_close(window:WebviewWindow,host:State<'_,Host>,handle:Uuid)->Reply<()>{
    run(window,host.inner().clone(),move|h|h.close(handle)).await
}
#[tauri::command]
async fn desktop_start_transform(window:WebviewWindow,host:State<'_,Host>,handle:Uuid,source:u32,target:u32,xy:Vec<f64>)->Reply<Uuid>{
    run(window,host.inner().clone(),move|h|h.start(handle,source,target,xy)).await
}
#[tauri::command]
async fn desktop_poll(window:WebviewWindow,host:State<'_,Host>,handle:Uuid,id:Uuid)->Reply<Poll>{
    run(window,host.inner().clone(),move|h|h.poll(handle,id)).await
}
#[tauri::command]
async fn desktop_cancel(window:WebviewWindow,host:State<'_,Host>,handle:Uuid,id:Uuid)->Reply<()>{
    run(window,host.inner().clone(),move|h|h.cancel(handle,id)).await
}

// A separate bounded binary read operation; never returns the full dataset.
#[tauri::command]
async fn desktop_viewport(window:WebviewWindow,host:State<'_,Host>,handle:Uuid,bbox:[f64;4])->Reply<tauri::ipc::Response>{
    let data=run(window,host.inner().clone(),move|h|h.viewport(handle,bbox)).await?;
    let bytes=serde_json::to_vec(&data).map_err(|_|"INVALID_LOCAL_DATA".to_owned())?;
    if bytes.len()>2*1024*1024{return Err("RESOURCE_LIMIT".into())}
    Ok(tauri::ipc::Response::new(bytes))
}
#[tauri::command]
async fn desktop_attributes(window:WebviewWindow,host:State<'_,Host>,handle:Uuid,after:String,limit:u32,contains:String)->Reply<Value>{
    run(window,host.inner().clone(),move|h|h.attributes(handle,after,limit,contains)).await
}
#[tauri::command]
async fn desktop_feature(window:WebviewWindow,host:State<'_,Host>,handle:Uuid,id:Uuid)->Reply<Value>{
    run(window,host.inner().clone(),move|h|h.feature(handle,id)).await
}
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct EditInput {command:Uuid,id:Uuid,revision:String,kind:Option<String>,xy:Vec<f64>,attributes:Value}
#[tauri::command]
async fn desktop_edit(window:WebviewWindow,host:State<'_,Host>,handle:Uuid,edit:EditInput)->Reply<()>{
    run(window,host.inner().clone(),move|h|h.edit(handle,edit.command,edit.id,edit.revision,edit.kind,edit.xy,edit.attributes)).await
}
#[tauri::command]
async fn desktop_update_attributes(window:WebviewWindow,host:State<'_,Host>,handle:Uuid,id:Uuid,revision:String,attributes:Value)->Reply<()>{
    run(window,host.inner().clone(),move|h|h.update_attributes(handle,id,revision,attributes)).await
}
#[tauri::command]
async fn desktop_history(window:WebviewWindow,host:State<'_,Host>,handle:Uuid,redo:bool)->Reply<bool>{
    run(window,host.inner().clone(),move|h|h.history(handle,redo)).await
}
#[tauri::command]
async fn desktop_checkpoint(window:WebviewWindow,host:State<'_,Host>,handle:Uuid)->Reply<()>{
    run(window,host.inner().clone(),move|h|h.checkpoint(handle)).await
}
#[tauri::command]
async fn desktop_import_geojson(window:WebviewWindow,host:State<'_,Host>,handle:Uuid)->Reply<Option<u32>>{
    let app=window.app_handle().clone();
    run(window,host.inner().clone(),move|h|{
        h.generation(handle)?;
        let Some(file)=app.dialog().file().add_filter("GeoJSON",&["geojson","json"]).blocking_pick_file() else{return Ok(None)};
        let path=file.into_path().map_err(|_|"INVALID_STORAGE")?;
        h.import_geojson(handle,&path).map(Some)
    }).await
}
#[tauri::command]
async fn desktop_export(window:WebviewWindow,host:State<'_,Host>,handle:Uuid,format:String)->Reply<bool>{
    let app=window.app_handle().clone();
    run(window,host.inner().clone(),move|h|{
        h.generation(handle)?;
        let (label,ext)=match format.as_str(){"gpkg"=>("GeoPackage snapshot","gpkg"),"geojson"=>("GeoJSON","geojson"),_=>return Err("UNSUPPORTED_FORMAT")};
        let Some(file)=app.dialog().file().add_filter(label,&[ext]).set_file_name(format!("workspace.{ext}")).blocking_save_file() else{return Ok(false)};
        let path=file.into_path().map_err(|_|"INVALID_STORAGE")?;
        if path.extension().and_then(|x|x.to_str())!=Some(ext){return Err("UNSUPPORTED_FORMAT")}
        if ext=="gpkg"{h.snapshot(handle,&path)?}else{h.export_geojson(handle,&path)?}
        Ok(true)
    }).await
}

#[tauri::command]
async fn desktop_open_raster(window:WebviewWindow,host:State<'_,Host>,handle:Uuid)->Reply<Option<Raster>>{
    let app=window.app_handle().clone();
    run(window,host.inner().clone(),move|h|{
        h.generation(handle)?;
        let Some(file)=app.dialog().file().add_filter("PNG MBTiles",&["mbtiles"]).blocking_pick_file() else{return Ok(None)};
        let path=file.into_path().map_err(|_|"INVALID_STORAGE")?;
        h.open_raster(handle,&path).map(Some)
    }).await
}
#[tauri::command]
async fn desktop_close_raster(window:WebviewWindow,host:State<'_,Host>,handle:Uuid)->Reply<()>{
    run(window,host.inner().clone(),move|h|h.close_raster(handle)).await
}
// Logical ovg:// resource route, transported as binary IPC until OS custom
// scheme origin/session qualification completes. No generic file reads.
#[tauri::command]
async fn desktop_resource(window:WebviewWindow,host:State<'_,Host>,uri:String,request_id:Uuid)->Reply<tauri::ipc::Response>{
    trusted(&window)?;
    let read=ReadPermit::acquire(request_id)?;
    let req=protocol::parse(&uri).map_err(|_|"INVALID_RESOURCE".to_owned())?;
    let handle=Uuid::parse_str(req.workspace).map_err(|_|"INVALID_RESOURCE".to_owned())?;
    let source=Uuid::parse_str(req.source).map_err(|_|"INVALID_RESOURCE".to_owned())?;
    let revision=req.revision;
    let resource=req.resource;
    let b=run(window,host.inner().clone(),move|h|{
        if read.token.load(Ordering::Acquire){return Err("CANCELLED")}
        let data=match resource{
        protocol::Resource::RasterTile{z,x,y} if revision==0=>h.raster_tile(handle,source,z,x,y),
        protocol::Resource::VectorTile{z,x,y}=>h.vector_tile_cancel(handle,source,revision,z,x,y,&read.token),
        protocol::Resource::TileJson=>h.tilejson(handle,source,revision),
        protocol::Resource::CadChunk(_) | protocol::Resource::PointNode(_) | protocol::Resource::Asset if revision==0=>h.asset_resource(handle,source,resource),
        _=>Err("NOT_AVAILABLE")
        }?;
        if read.token.load(Ordering::Acquire){return Err("CANCELLED")}
        Ok(data)
    }).await?;
    Ok(tauri::ipc::Response::new(b))
}
#[tauri::command]
async fn desktop_import_gpkg(window:WebviewWindow,host:State<'_,Host>,handle:Uuid)->Reply<Option<u32>>{
    let app=window.app_handle().clone();
    run(window,host.inner().clone(),move|h|{
        h.generation(handle)?;
        let Some(file)=app.dialog().file().add_filter("GeoPackage XY WGS84",&["gpkg"]).blocking_pick_file() else{return Ok(None)};
        let path=file.into_path().map_err(|_|"INVALID_STORAGE")?;
        h.import_gpkg(handle,&path).map(Some)
    }).await
}
#[tauri::command]
async fn desktop_generation(window:WebviewWindow,host:State<'_,Host>,handle:Uuid)->Reply<String>{
    run(window,host.inner().clone(),move|h|h.generation(handle)).await
}
#[tauri::command]
async fn desktop_reshape(window:WebviewWindow,host:State<'_,Host>,handle:Uuid,id:Uuid,revision:String,geometry:Value)->Reply<()>{
    run(window,host.inner().clone(),move|h|h.reshape(handle,id,revision,geometry)).await
}
#[tauri::command]
async fn desktop_open_asset(window:WebviewWindow,host:State<'_,Host>,handle:Uuid,kind:String,epsg:Option<u32>)->Reply<Option<AssetInfo>>{
    let app=window.app_handle().clone();
    run(window,host.inner().clone(),move|h|{
        h.generation(handle)?;
        let (label,ext)=match kind.as_str(){"cad"=>("DXF LINE/POINT XY","dxf"),"las"=>("LAS uncompressed","las"),"laz"=>("LAZ compressed","laz"),"copc"=>("COPC point cloud","laz"),"glb"=>("Self-contained GLB","glb"),_=>return Err("UNSUPPORTED_FORMAT")};
        let Some(file)=app.dialog().file().add_filter(label,&[ext]).blocking_pick_file() else{return Ok(None)};
        let path=file.into_path().map_err(|_|"INVALID_STORAGE")?;
        h.open_asset(handle,&path,&kind,epsg).map(Some)
    }).await
}
#[tauri::command]
async fn desktop_close_asset(window:WebviewWindow,host:State<'_,Host>,handle:Uuid,id:Uuid)->Reply<()>{
    run(window,host.inner().clone(),move|h|h.close_asset(handle,id)).await
}
#[tauri::command]
async fn desktop_cad_viewport(window:WebviewWindow,host:State<'_,Host>,handle:Uuid,id:Uuid,bbox:[f64;4])->Reply<tauri::ipc::Response>{
    let b=run(window,host.inner().clone(),move|h|h.cad_viewport(handle,id,bbox)).await?;
    Ok(tauri::ipc::Response::new(b))
}
#[tauri::command]
async fn desktop_restore_snapshot(window:WebviewWindow,host:State<'_,Host>)->Reply<Option<Project>>{
    let app=window.app_handle().clone();
    run(window,host.inner().clone(),move|h|{
        let Some(file)=app.dialog().file().add_filter("OpenVietGIS standalone snapshot",&["gpkg"]).blocking_pick_file() else{return Ok(None)};
        let path=file.into_path().map_err(|_|"INVALID_STORAGE")?;
        h.restore_snapshot(&path).map(Some)
    }).await
}
#[tauri::command]
async fn desktop_inspect_gpkg(window:WebviewWindow,host:State<'_,Host>,handle:Uuid)->Reply<Option<Value>>{
    let app=window.app_handle().clone();
    run(window,host.inner().clone(),move|h|{
        h.generation(handle)?;
        let Some(file)=app.dialog().file().add_filter("GeoPackage",&["gpkg"]).blocking_pick_file() else{return Ok(None)};
        let path=file.into_path().map_err(|_|"INVALID_STORAGE")?;
        h.inspect_gpkg(handle,&path).map(Some)
    }).await
}
#[tauri::command]
async fn desktop_import_layer(window:WebviewWindow,host:State<'_,Host>,handle:Uuid,grant:Uuid,table:String)->Reply<u32>{
    if table.len()>128{return Err("RESOURCE_LIMIT".into())}
    run(window,host.inner().clone(),move|h|h.import_layer(handle,grant,&table)).await
}
#[tauri::command]
async fn desktop_copc_query(window:WebviewWindow,host:State<'_,Host>,handle:Uuid,id:Uuid,bounds:[f64;6],level:i32)->Reply<tauri::ipc::Response>{
    let b=run(window,host.inner().clone(),move|h|h.copc_query(handle,id,bounds,level)).await?;
    Ok(tauri::ipc::Response::new(b))
}
#[tauri::command]
async fn desktop_list_layers(window:WebviewWindow,host:State<'_,Host>,handle:Uuid)->Reply<Vec<LayerInfo>>{
    run(window,host.inner().clone(),move|h|h.list_layers(handle)).await
}
#[tauri::command]
async fn desktop_create_layer(window:WebviewWindow,host:State<'_,Host>,handle:Uuid,name:String,grant:Option<Uuid>,table:Option<String>)->Reply<LayerInfo>{
    let catalog=match(grant,table){(Some(g),Some(t)) if t.len()<=128=>Some((g,t)),(None,None)=>None,_=>return Err("INVALID_LAYER".into())};
    run(window,host.inner().clone(),move|h|h.create_layer(handle,name,catalog)).await
}
#[tauri::command]
async fn desktop_open_layer(window:WebviewWindow,host:State<'_,Host>,handle:Uuid,layer:Uuid)->Reply<Opened>{
    run(window,host.inner().clone(),move|h|h.open_layer(handle,layer)).await
}
fn main(){
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        // Separate read plane; no data provider is qualified in W1. Reject every
        // valid resource as unavailable, never return synthetic project data.
        .register_uri_scheme_protocol("ovg", |_ctx, req| {
            use tauri::http::{Method, Response, StatusCode};
            let path = req.uri().path();
            let canonical = format!("ovg://v1{}", if path.starts_with("/v1/") { &path[3..] } else { path });
            let valid = req.method()==Method::GET && req.uri().query().is_none()
                && protocol::parse(&canonical).is_ok();
            Response::builder().status(if valid {StatusCode::NOT_FOUND} else {StatusCode::FORBIDDEN})
                .header("Cache-Control","no-store").header("Content-Type","application/octet-stream")
                .body(Vec::<u8>::new()).expect("static response")
        })
        .setup(|app|{
            let root=app.path().app_local_data_dir()?.join("standalone");
            app.manage(Host::new(root).map_err(std::io::Error::other)?);
            let new=tauri::menu::MenuItemBuilder::with_id("new","New local workspace").accelerator("CmdOrCtrl+N").build(app)?;
            let close=tauri::menu::MenuItemBuilder::with_id("close-workspace","Close workspace").accelerator("CmdOrCtrl+Shift+W").build(app)?;
            let file=tauri::menu::SubmenuBuilder::new(app,"File").items(&[&new,&close]).build()?;
            app.set_menu(tauri::menu::MenuBuilder::new(app).item(&file).build()?)?;
            Ok(())
        })
        .on_menu_event(|app,event|{
            let id: &str=event.id().as_ref();
            if matches!(id,"new"|"close-workspace"){
                if let Some(w)=app.get_webview_window("main"){let _=w.emit("desktop-menu",id);}
            }
        })
        .on_window_event(|window,event|{
            if window.label()=="main" && matches!(event,tauri::WindowEvent::Destroyed){
                if let Some(host)=window.try_state::<Host>(){let _=host.shutdown();}
            }
        })
        .invoke_handler(tauri::generate_handler![desktop_capabilities,desktop_list,desktop_create,desktop_open,desktop_close,desktop_start_transform,desktop_poll,desktop_cancel,desktop_viewport,desktop_attributes,desktop_feature,desktop_edit,desktop_update_attributes,desktop_history,desktop_checkpoint,desktop_import_geojson,desktop_export,desktop_open_raster,desktop_close_raster,desktop_resource,desktop_import_gpkg,desktop_generation,desktop_reshape,desktop_open_asset,desktop_close_asset,desktop_cad_viewport,desktop_cancel_resource,desktop_restore_snapshot,desktop_inspect_gpkg,desktop_import_layer,desktop_copc_query,desktop_list_layers,desktop_create_layer,desktop_open_layer])
        .run(tauri::generate_context!()).expect("Desktop host failed");
}
