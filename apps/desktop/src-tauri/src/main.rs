#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
mod host;
#[path="../../../../infra/desktop/wave0/protocol.rs"]
mod protocol;
use host::{Host, Opened, Poll, Project};
use serde::Serialize;
use tauri::{Emitter, Manager, State, WebviewWindow};
use uuid::Uuid;

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
    tauri::async_runtime::spawn_blocking(move||f(host)).await.map_err(|_|"SESSION_FAILED".to_owned())?.map_err(str::to_owned)
}
#[derive(Serialize)]
struct Capabilities { protocol_version:u32, standalone:bool, managed:bool, cloud:bool, local_tiles:bool, transform:bool }
#[tauri::command]
fn desktop_capabilities(window:WebviewWindow)->Reply<Capabilities>{
    trusted(&window)?;
    Ok(Capabilities{protocol_version:1,standalone:true,managed:false,cloud:false,local_tiles:false,transform:true})
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
fn main(){
    tauri::Builder::default()
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
        .invoke_handler(tauri::generate_handler![desktop_capabilities,desktop_list,desktop_create,desktop_open,desktop_close,desktop_start_transform,desktop_poll,desktop_cancel])
        .run(tauri::generate_context!()).expect("Desktop host failed");
}
