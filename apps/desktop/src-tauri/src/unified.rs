//! Explicit experimental projects, separate from legacy LocalStore projects.
//! Only app-private files; external migration exports are not silently activated.
use super::*;
use ovg_project_store::{Store,Batch,Edit,Image,Link,virtual_query::{Plan,Cursor}};
use std::{fs,path::PathBuf};
use serde::{Deserialize,Serialize};
#[derive(Clone)]
pub struct Unified{root:PathBuf,open:Arc<Mutex<HashMap<Uuid,(Uuid,Store)>>>}
fn err(e:ovg_project_store::Error)->&'static str{match e{
 ovg_project_store::Error::Conflict=>"REVISION_CONFLICT",ovg_project_store::Error::Reused=>"OPERATION_ID_REUSED",ovg_project_store::Error::Relation=>"RELATION_CONSTRAINT",ovg_project_store::Error::Limit=>"RESOURCE_LIMIT",ovg_project_store::Error::Unsupported=>"UNSUPPORTED_STORE",_=>"UNIFIED_STORE_ERROR"
}}
type UResult<T>=Result<T,&'static str>;
#[derive(Serialize)]pub struct Project{pub id:Uuid}
#[derive(Serialize)]pub struct Opened{pub id:Uuid,pub handle:Uuid}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeometryEdit{layer:Uuid,feature:Uuid,expected:String,geometry:Option<Value>,attributes:Value}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Input{operation:Uuid,expected_project:String,edits:Vec<GeometryEdit>,links:Vec<Link>}
fn number(s:&str)->UResult<u64>{s.parse().map_err(|_|"INVALID_REVISION")}
fn geo(v:Value)->UResult<Vec<u8>>{
 let kind=v["type"].as_str().ok_or("INVALID_GEOMETRY")?;
 if !matches!(kind,"Point"|"MultiPoint"|"LineString"|"MultiLineString"|"Polygon"|"MultiPolygon"){return Err("UNSUPPORTED_GEOMETRY")}
 fn xy(v:&Value,d:usize)->bool{if d>5{return false}let Some(a)=v.as_array()else{return false};if a.is_empty(){return false}if a[0].is_number(){return a.len()==2&&a.iter().all(|x|x.as_f64().is_some_and(f64::is_finite))}a.iter().all(|x|xy(x,d+1))}
 if !xy(&v["coordinates"],0){return Err("INVALID_GEOMETRY")}
 let g=ovg_spatial::parse_geojson(&v,&ovg_spatial::ValidationLimits::default()).map_err(|_|"INVALID_GEOMETRY")?;
 ovg_spatial::encode_wkb(&g,None).map_err(|_|"INVALID_GEOMETRY")
}
impl Unified{
 pub fn new(root:PathBuf)->UResult<Self>{fs::create_dir_all(&root).map_err(|_|"STORAGE_UNAVAILABLE")?;if !fs::symlink_metadata(&root).map_err(|_|"STORAGE_UNAVAILABLE")?.file_type().is_dir(){return Err("INVALID_STORAGE")}Ok(Self{root,open:Arc::new(Mutex::new(HashMap::new()))})}
 fn path(&self,id:Uuid)->PathBuf{self.root.join(format!("{id}.sqlite"))}
 pub fn list(&self)->UResult<Vec<Project>>{
  let mut out=Vec::new();for e in fs::read_dir(&self.root).map_err(|_|"STORAGE_UNAVAILABLE")?{
   let e=e.map_err(|_|"STORAGE_UNAVAILABLE")?;if !e.file_type().map_err(|_|"STORAGE_UNAVAILABLE")?.is_file(){continue}
   let n=e.file_name();let Some(n)=n.to_str().and_then(|n|n.strip_suffix(".sqlite"))else{continue};
   if let Ok(id)=Uuid::parse_str(n){if id.to_string()==n{out.push(Project{id})}}
   if out.len()>128{return Err("RESOURCE_LIMIT")}
  }out.sort_by_key(|p|p.id);Ok(out)
 }
 pub fn create(&self)->UResult<Project>{
  let _lock=self.open.lock().map_err(|_|"SESSION_FAILED")?;
  if self.list()?.len()>=128{return Err("RESOURCE_LIMIT")}
  let id=Uuid::new_v4();let stage=tempfile::tempdir_in(&self.root).map_err(|_|"STORAGE_UNAVAILABLE")?;
  let p=stage.path().join("new.sqlite");let store=Store::create(&p).map_err(err)?;
  store.add_layer(Uuid::new_v4(),"Default",0).map_err(err)?;store.checkpoint().map_err(err)?;drop(store);
  fs::hard_link(&p,self.path(id)).map_err(|_|"STORAGE_UNAVAILABLE")?;Ok(Project{id})
 }
 pub fn open(&self,id:Uuid)->UResult<Opened>{
  let mut s=self.open.lock().map_err(|_|"SESSION_FAILED")?;
  if s.len()>=4||s.values().any(|(i,_)|*i==id){return Err("RESOURCE_LIMIT")}
  let p=self.path(id);
  for suffix in ["-wal","-shm","-journal"]{let mut n=p.as_os_str().to_os_string();n.push(suffix);match fs::symlink_metadata(n){Ok(m) if !m.file_type().is_file()=>return Err("INVALID_STORAGE"),Err(e) if e.kind()!=std::io::ErrorKind::NotFound=>return Err("INVALID_STORAGE"),_=>{}}}
  let store=Store::reopen(&p).map_err(err)?;if !store.integrity().map_err(err)?{return Err("INVALID_STORAGE")}
  let handle=Uuid::new_v4();s.insert(handle,(id,store));Ok(Opened{id,handle})
 }
 pub fn close(&self,h:Uuid)->UResult<()>{let mut s=self.open.lock().map_err(|_|"SESSION_FAILED")?;s.get(&h).ok_or("STALE_HANDLE")?.1.checkpoint().map_err(err)?;s.remove(&h);Ok(())}
 pub fn with<T>(&self,h:Uuid,f:impl FnOnce(&Store)->UResult<T>)->UResult<T>{let s=self.open.lock().map_err(|_|"SESSION_FAILED")?;f(&s.get(&h).ok_or("STALE_HANDLE")?.1)}
 pub fn apply(&self,h:Uuid,input:Input)->UResult<Value>{
  if input.edits.len()+input.links.len()>64{return Err("RESOURCE_LIMIT")}
  let mut edits=Vec::new();let mut size=0usize;
  for e in input.edits{
   size=size.saturating_add(e.attributes.to_string().len()+e.geometry.as_ref().map_or(0,|v|v.to_string().len()));
   if size>2*1024*1024{return Err("RESOURCE_LIMIT")}
   let image=e.geometry.map(|v|geo(v).map(|wkb|Image{wkb,attributes:e.attributes})).transpose()?;
   edits.push(Edit{layer:e.layer,feature:e.feature,expected:number(&e.expected)?,image});
  }
  self.with(h,|s|{let r=s.apply(&Batch{operation:input.operation,expected_project:number(&input.expected_project)?,edits,links:input.links}).map_err(err)?;
   Ok(serde_json::json!({"revision":r.revision.to_string(),"features":r.features.iter().map(|(l,f,r)|serde_json::json!([l,f,r.to_string()])).collect::<Vec<_>>() }))})
 }
 pub fn query(&self,h:Uuid,plan:Plan,cursor:Option<Cursor>,expected:String)->UResult<Value>{
  self.with(h,|s|{
   let page=s.virtual_page(&plan,cursor.as_ref(),100,number(&expected)?).map_err(err)?;
   let mut features=Vec::new();let mut bytes=0;
   for r in page.rows{
    let g=ovg_spatial::parse_xy_wkb(&r.image.wkb,&ovg_spatial::ValidationLimits::default()).map_err(|_|"INVALID_GEOMETRY")?;
    let f=serde_json::json!({"type":"Feature","id":r.id,"geometry":ovg_spatial::to_geojson(&g.geometry),"properties":{"revision":r.revision.to_string(),"attributes":r.image.attributes}});
    bytes+=f.to_string().len();if bytes>4*1024*1024{return Err("RESOURCE_LIMIT")}features.push(f);
   }
   Ok(serde_json::json!({"data":{"type":"FeatureCollection","features":features},"next":page.next.map(|c|serde_json::json!({"revision":c.revision.to_string(),"plan_digest":c.plan_digest,"after":c.after.to_string()})),"scanned":page.scanned}))
  })
 }
}
async fn work<T:Send+'static>(window:WebviewWindow,u:Unified,f:impl FnOnce(Unified)->UResult<T>+Send+'static)->Reply<T>{
 trusted(&window)?;let permit=Permit::acquire()?;
 tauri::async_runtime::spawn_blocking(move||{let _p=permit;f(u)}).await.map_err(|_|"SESSION_FAILED".to_owned())?.map_err(str::to_owned)
}
#[tauri::command]pub async fn unified_list(window:WebviewWindow,u:State<'_,Unified>)->Reply<Vec<Project>>{work(window,u.inner().clone(),|s|s.list()).await}
#[tauri::command]pub async fn unified_create(window:WebviewWindow,u:State<'_,Unified>)->Reply<Project>{work(window,u.inner().clone(),|s|s.create()).await}
#[tauri::command]pub async fn unified_open(window:WebviewWindow,u:State<'_,Unified>,id:Uuid)->Reply<Opened>{work(window,u.inner().clone(),move|s|s.open(id)).await}
#[tauri::command]pub async fn unified_close(window:WebviewWindow,u:State<'_,Unified>,handle:Uuid)->Reply<()>{work(window,u.inner().clone(),move|s|s.close(handle)).await}
#[tauri::command]pub async fn unified_catalog(window:WebviewWindow,u:State<'_,Unified>,handle:Uuid)->Reply<ovg_project_store::catalog::Catalog>{work(window,u.inner().clone(),move|s|s.with(handle,|s|s.catalog().map_err(err))).await}
#[tauri::command]pub async fn unified_add_layer(window:WebviewWindow,u:State<'_,Unified>,handle:Uuid,name:String,expected:String)->Reply<String>{work(window,u.inner().clone(),move|s|s.with(handle,|s|Ok(s.add_layer(Uuid::new_v4(),&name,number(&expected)?).map_err(err)?.to_string()))).await}
#[tauri::command]pub async fn unified_add_relation(window:WebviewWindow,u:State<'_,Unified>,handle:Uuid,source:Uuid,target:Uuid,kind:String,expected:String)->Reply<String>{work(window,u.inner().clone(),move|s|s.with(handle,|s|Ok(s.add_relation(Uuid::new_v4(),source,target,&kind,number(&expected)?).map_err(err)?.to_string()))).await}
#[tauri::command]pub async fn unified_apply(window:WebviewWindow,u:State<'_,Unified>,handle:Uuid,input:Input)->Reply<Value>{work(window,u.inner().clone(),move|s|s.apply(handle,input)).await}
#[derive(Deserialize)]pub struct InputCursor{revision:String,plan_digest:String,after:String}
#[tauri::command]pub async fn unified_query(window:WebviewWindow,u:State<'_,Unified>,handle:Uuid,plan:Plan,cursor:Option<InputCursor>,expected:String)->Reply<Value>{
 let cursor=cursor.map(|c|->Reply<Cursor>{Ok(Cursor{revision:number(&c.revision)?,plan_digest:c.plan_digest,after:c.after.parse().map_err(|_|"INVALID_CURSOR")?})}).transpose()?;
 work(window,u.inner().clone(),move|s|s.query(handle,plan,cursor,expected)).await
}
#[tauri::command]pub async fn unified_compensate(window:WebviewWindow,u:State<'_,Unified>,handle:Uuid,operation:Uuid,expected:String)->Reply<String>{work(window,u.inner().clone(),move|s|s.with(handle,|s|Ok(s.compensate_latest(operation,number(&expected)?).map_err(err)?.revision.to_string()))).await}
#[tauri::command]pub async fn unified_schema(window:WebviewWindow,u:State<'_,Unified>,handle:Uuid,layer:Uuid,fields:Vec<ovg_mobile_engine::attributes::Field>,expected:String)->Reply<String>{work(window,u.inner().clone(),move|s|s.with(handle,|s|Ok(s.set_schema(layer,&fields,number(&expected)?).map_err(err)?.to_string()))).await}
