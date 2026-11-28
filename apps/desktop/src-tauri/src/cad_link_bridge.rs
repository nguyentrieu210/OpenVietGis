//! Tauri boundary for linked native CAD operations.
use super::*;
use ovg_project_store::cad_link::CadLink;
use super::link_logic::{register,reconcile};
#[tauri::command]
pub async fn native_cad_link_status(window:WebviewWindow,u:State<'_,Unified>,handle:Uuid,layer:Uuid,expected:String)->Reply<Option<CadLink>>{
 work(window,u.inner().clone(),move|s|s.with(handle,|store|store.cad_link(layer,unified::number(&expected)?).map_err(|_|"STORE_QUERY_FAILED"))).await
}
#[tauri::command]
pub async fn native_cad_link_register(window:WebviewWindow,u:State<'_,Unified>,handle:Uuid,layer:Uuid,asset:Uuid,expected:String,automatic:bool,assert_wgs84:bool)->Reply<String>{
 if !assert_wgs84{return Err("EXPLICIT_CRS_REQUIRED".into())}
 work(window,u.inner().clone(),move|s|s.with(handle,|store|Ok(register(store,layer,asset,unified::number(&expected)?,automatic)?.to_string()))).await
}
#[tauri::command]
pub async fn native_cad_link_automatic(window:WebviewWindow,u:State<'_,Unified>,handle:Uuid,layer:Uuid,expected:String,automatic:bool)->Reply<String>{
 work(window,u.inner().clone(),move|s|s.with(handle,|store|Ok(store.cad_link_automatic(layer,unified::number(&expected)?,automatic).map_err(|_|"CAD_LINK_UPDATE_FAILED")?.to_string()))).await
}
#[tauri::command]
pub async fn native_cad_link_reconcile(window:WebviewWindow,u:State<'_,Unified>,handle:Uuid,layer:Uuid,expected:String,operation:Uuid,automatic:bool)->Reply<Value>{
 work(window,u.inner().clone(),move|s|s.with(handle,|store|reconcile(store,layer,unified::number(&expected)?,operation,automatic))).await
}
