//! Explicit experimental projects, separate from legacy LocalStore projects.
//! Only app-private files; external migration exports are not silently activated.
use super::*;
use ovg_project_store::{Store,Batch,Edit,Image,Link,virtual_query::{Plan,Cursor}};
use std::{fs,path::PathBuf};
use serde::{Deserialize,Serialize};
#[derive(Clone)]
pub struct Unified{root:PathBuf,open:Arc<Mutex<HashMap<Uuid,(Uuid,Store)>>>,assets:Arc<Mutex<HashMap<(Uuid,Uuid),crate::unified_assets::Entry>>>}
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
pub(crate) fn number(s:&str)->UResult<u64>{s.parse().map_err(|_|"INVALID_REVISION")}
fn geo(v:Value)->UResult<Vec<u8>>{
 let kind=v["type"].as_str().ok_or("INVALID_GEOMETRY")?;
 if !matches!(kind,"Point"|"MultiPoint"|"LineString"|"MultiLineString"|"Polygon"|"MultiPolygon"){return Err("UNSUPPORTED_GEOMETRY")}
 fn xy(v:&Value,d:usize)->bool{if d>5{return false}let Some(a)=v.as_array()else{return false};if a.is_empty(){return false}if a[0].is_number(){return a.len()==2&&a.iter().all(|x|x.as_f64().is_some_and(f64::is_finite))}a.iter().all(|x|xy(x,d+1))}
 if !xy(&v["coordinates"],0){return Err("INVALID_GEOMETRY")}
 let g=ovg_spatial::parse_geojson(&v,&ovg_spatial::ValidationLimits::default()).map_err(|_|"INVALID_GEOMETRY")?;
 ovg_spatial::encode_wkb(&g,None).map_err(|_|"INVALID_GEOMETRY")
}
impl Unified{
 pub fn new(root:PathBuf)->UResult<Self>{fs::create_dir_all(&root).map_err(|_|"STORAGE_UNAVAILABLE")?;if !fs::symlink_metadata(&root).map_err(|_|"STORAGE_UNAVAILABLE")?.file_type().is_dir(){return Err("INVALID_STORAGE")}Ok(Self{root,open:Arc::new(Mutex::new(HashMap::new())),assets:Arc::new(Mutex::new(HashMap::new()))})}
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
 pub fn restore(&self,path:&std::path::Path)->UResult<Project>{
  let _lock=self.open.lock().map_err(|_|"SESSION_FAILED")?;
  if self.list()?.len()>=128{return Err("RESOURCE_LIMIT")}
  let id=Uuid::new_v4();
  Store::restore_copy(path,&self.path(id),&AtomicBool::new(false)).map_err(err)?;Ok(Project{id})
 }
 pub fn shutdown(&self)->UResult<()>{let mut s=self.open.lock().map_err(|_|"SESSION_FAILED")?;for (_,store) in s.values(){store.checkpoint().map_err(err)?;}s.clear();self.assets.lock().map_err(|_|"SESSION_FAILED")?.clear();Ok(())}
 pub fn close(&self,h:Uuid)->UResult<()>{let mut s=self.open.lock().map_err(|_|"SESSION_FAILED")?;s.get(&h).ok_or("STALE_HANDLE")?.1.checkpoint().map_err(err)?;s.remove(&h);self.assets.lock().map_err(|_|"SESSION_FAILED")?.retain(|(handle,_),_|*handle!=h);Ok(())}
 pub fn with<T>(&self,h:Uuid,f:impl FnOnce(&Store)->UResult<T>)->UResult<T>{let s=self.open.lock().map_err(|_|"SESSION_FAILED")?;f(&s.get(&h).ok_or("STALE_HANDLE")?.1)}
 pub fn attach_asset(&self,h:Uuid,path:&std::path::Path,kind:&str,epsg:Option<u32>,expected:u64)->UResult<ovg_project_store::asset_store::AssetMeta>{
  use std::io::Read;
  self.with(h,|s|{
   let stage=tempfile::tempdir().map_err(|_|"STORAGE_UNAVAILABLE")?;
   let p=stage.path().join(crate::unified_assets::filename(kind)?);
   if kind=="mbtiles"{
    let snap=crate::gpkg_import::sqlite_snapshot(path,512*1024*1024)?;
    fs::copy(snap.path(),&p).map_err(|_|"STORAGE_UNAVAILABLE")?;
   }else{
    let m=fs::symlink_metadata(path).map_err(|_|"INVALID_FILE")?;
    if !m.file_type().is_file()||m.len()>512*1024*1024{return Err("RESOURCE_LIMIT")}
    let mut input=fs::File::open(path).map_err(|_|"INVALID_FILE")?.take(512*1024*1024+1);
    let mut out=fs::OpenOptions::new().write(true).create_new(true).open(&p).map_err(|_|"STORAGE_UNAVAILABLE")?;
    if std::io::copy(&mut input,&mut out).map_err(|_|"STORAGE_UNAVAILABLE")?!=m.len(){return Err("SOURCE_CHANGED")}
    out.sync_all().map_err(|_|"STORAGE_UNAVAILABLE")?;
   }
   // Validate exactly the staged bytes using existing native adapter.
   let _validated=crate::unified_assets::Entry::open(&p,kind,epsg)?;
   s.attach_asset(Uuid::new_v4(),kind,epsg,&p,expected,&AtomicBool::new(false)).map_err(err)
  })
 }
 pub fn reopen_asset(&self,h:Uuid,id:Uuid)->UResult<crate::unified_assets::Info>{
  self.with(h,|s|{
   let mut a=self.assets.lock().map_err(|_|"SESSION_FAILED")?;
   if let Some(entry)=a.get(&(h,id)){return Ok(entry.info.clone())}
   if a.keys().filter(|(k,_)|*k==h).count()>=3||a.len()>=12{return Err("RESOURCE_LIMIT")}
   let meta=s.assets().map_err(err)?.into_iter().find(|m|m.id==id).ok_or("STALE_HANDLE")?;
   let stage=tempfile::tempdir().map_err(|_|"STORAGE_UNAVAILABLE")?;
   let p=stage.path().join(crate::unified_assets::filename(&meta.kind)?);
   s.materialize_asset(id,&p,&AtomicBool::new(false)).map_err(err)?;
   let mut entry=crate::unified_assets::Entry::open(&p,&meta.kind,meta.epsg)?;
   entry.info.id=id;let info=entry.info.clone();a.insert((h,id),entry);Ok(info)
  })
 }
 pub fn detach_asset(&self,h:Uuid,id:Uuid,expected:u64)->UResult<u64>{
  self.with(h,|s|{let mut a=self.assets.lock().map_err(|_|"SESSION_FAILED")?;
   let revision=s.detach_asset(id,expected).map_err(err)?;a.remove(&(h,id));Ok(revision)
  })
 }
 pub fn import_ndjson(&self,h:Uuid,path:&std::path::Path,name:&str,expected:u64)->UResult<ovg_project_store::bulk_import::BulkReceipt>{
  use std::io::{BufRead,BufReader,Read};
  use ovg_project_store::{bulk_import::BulkRow,Error};
  let m=fs::symlink_metadata(path).map_err(|_|"INVALID_FILE")?;
  if !m.file_type().is_file()||m.len()>128*1024*1024{return Err("RESOURCE_LIMIT")}
  let file=fs::File::open(path).map_err(|_|"INVALID_FILE")?;
  let mut reader=BufReader::new(file.take(128*1024*1024+1));
  let mut used=0u64;let mut done=false;
  let rows=std::iter::from_fn(||{
   if done{return None}
   let mut line=Vec::new();
   let n=match (&mut reader).take(2*1024*1024+1).read_until(b'\n',&mut line){Ok(n)=>n,Err(_)=>{done=true;return Some(Err(Error::Storage))}};
   if n==0{done=true;return if used==m.len(){None}else{Some(Err(Error::Conflict))}}
   used+=n as u64;
   if n>2*1024*1024||used>m.len(){done=true;return Some(Err(Error::Limit))}
   Some((||{
    let v:Value=serde_json::from_slice(&line).map_err(|_|Error::Invalid)?;
    if v["type"]!="Feature"||v.get("crs").is_some(){return Err(Error::Invalid)}
    let id=match v.get("id"){None=>Uuid::new_v4(),Some(Value::String(s))=>Uuid::parse_str(s).map_err(|_|Error::Invalid)?,_=>return Err(Error::Invalid)};
    let attributes=if v["properties"].is_null(){serde_json::json!({})}else{v["properties"].clone()};
    let wkb=geo(v["geometry"].clone()).map_err(|_|Error::Invalid)?;
    Ok(BulkRow{id,image:Image{wkb,attributes}})
   })())
  });
  self.with(h,|s|s.import_layer_stream(Uuid::new_v4(),Uuid::new_v4(),name,expected,rows,&AtomicBool::new(false)).map_err(err))
 }
 pub fn open_asset(&self,h:Uuid,path:&std::path::Path,kind:&str,epsg:Option<u32>)->UResult<crate::unified_assets::Info>{
  self.with(h,|_|{
   let mut assets=self.assets.lock().map_err(|_|"SESSION_FAILED")?;
   if assets.keys().filter(|(k,_)|*k==h).count()>=3||assets.len()>=12{return Err("RESOURCE_LIMIT")}
   let entry=crate::unified_assets::Entry::open(path,kind,epsg)?;
   let info=entry.info.clone();assets.insert((h,info.id),entry);Ok(info)
  })
 }
 pub fn close_asset(&self,h:Uuid,id:Uuid)->UResult<()>{
  self.with(h,|_|{self.assets.lock().map_err(|_|"SESSION_FAILED")?.remove(&(h,id)).ok_or("STALE_HANDLE")?;Ok(())})
 }
 pub fn with_asset<T>(&self,h:Uuid,id:Uuid,f:impl FnOnce(&mut crate::unified_assets::Entry)->UResult<T>)->UResult<T>{
  self.with(h,|_|{let mut a=self.assets.lock().map_err(|_|"SESSION_FAILED")?;f(a.get_mut(&(h,id)).ok_or("STALE_HANDLE")?)})
 }
 #[cfg(feature="native-ogr")]
 pub fn export_parquet(&self,h:Uuid,layer:Uuid,path:&std::path::Path,expected:u64)->UResult<u64>{
  self.with(h,|s|{
   let mut queue=std::collections::VecDeque::new();let mut after=0;let mut done=false;
   let rows=std::iter::from_fn(||{
    if queue.is_empty()&&!done{
     match s.query(layer,[-180.,-90.,180.,90.],after,100,expected){
      Ok(p)=>{queue.extend(p.rows);match p.next{Some(n) if n>after=>after=n,Some(_)=>{done=true;return Some(Err("INVALID_CURSOR"))},None=>done=true}},
      Err(e)=>{done=true;return Some(Err(err(e)))}
     }
    }
    queue.pop_front().map(|r|{
     let g=ovg_spatial::parse_xy_wkb(&r.image.wkb,&ovg_spatial::ValidationLimits::default()).map_err(|_|"INVALID_GEOMETRY")?;
     Ok((r.id.to_string(),ovg_spatial::to_geojson(&g.geometry),r.image.attributes))
    })
   });
   ovg_native_providers::ogr::export_parquet(path,rows)
  })
 }
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
 #[cfg(feature="native-ogr")]
 pub fn import_ogr(&self,h:Uuid,layer:Uuid,path:&std::path::Path,index:usize,expected:String)->UResult<Value>{
  let features=ovg_native_providers::ogr::read_atomic(path,index)?;
  let edits=features.into_iter().map(|f|GeometryEdit{layer,feature:Uuid::new_v4(),expected:"0".into(),geometry:Some(f["geometry"].clone()),attributes:f["properties"].clone()}).collect();
  self.apply(h,Input{operation:Uuid::new_v4(),expected_project:expected,edits,links:vec![]})
 }
 pub fn import_geojson(&self,h:Uuid,layer:Uuid,path:&std::path::Path,expected:String,operation:Uuid)->UResult<Value>{
  use std::io::Read;
  let meta=fs::symlink_metadata(path).map_err(|_|"INVALID_STORAGE")?;
  if !meta.file_type().is_file()||meta.len()>2*1024*1024{return Err("RESOURCE_LIMIT")}
  let mut b=Vec::new();fs::File::open(path).map_err(|_|"STORAGE_UNAVAILABLE")?.take(2*1024*1024+1).read_to_end(&mut b).map_err(|_|"STORAGE_UNAVAILABLE")?;
  if b.len()>2*1024*1024{return Err("RESOURCE_LIMIT")}
  let v:Value=serde_json::from_slice(&b).map_err(|_|"INVALID_GEOJSON")?;
  if v["type"]!="FeatureCollection"||v.get("crs").is_some(){return Err("WGS84_FEATURE_COLLECTION_REQUIRED")}
  let features=v["features"].as_array().ok_or("INVALID_GEOJSON")?;
  if features.is_empty()||features.len()>64{return Err("ATOMIC_IMPORT_LIMIT_64")}
  let mut edits=Vec::new();
  for f in features{
   if f["type"]!="Feature"||f["geometry"].is_null(){return Err("INVALID_GEOJSON")}
   let attrs=if f["properties"].is_null(){serde_json::json!({})}else{f["properties"].clone()};
   if !attrs.is_object(){return Err("INVALID_ATTRIBUTES")}
   edits.push(GeometryEdit{layer,feature:Uuid::new_v4(),expected:"0".into(),geometry:Some(f["geometry"].clone()),attributes:attrs});
  }
  self.apply(h,Input{operation,expected_project:expected,edits,links:vec![]})
 }
 pub fn export_geojson(&self,h:Uuid,layer:Uuid,path:&std::path::Path,expected:u64)->UResult<u64>{
  use std::io::Write;
  self.with(h,|s|{
   let stage=tempfile::NamedTempFile::new_in(path.parent().ok_or("INVALID_STORAGE")?).map_err(|_|"STORAGE_UNAVAILABLE")?;
   let mut f=stage.reopen().map_err(|_|"STORAGE_UNAVAILABLE")?;
   f.write_all(b"{\"type\":\"FeatureCollection\",\"features\":[").map_err(|_|"STORAGE_UNAVAILABLE")?;
   let mut after=0;let mut count=0u64;let mut bytes=0usize;
   loop{
    let page=s.query(layer,[-180.,-90.,180.,90.],after,100,expected).map_err(err)?;
    for r in page.rows{
     let g=ovg_spatial::parse_xy_wkb(&r.image.wkb,&ovg_spatial::ValidationLimits::default()).map_err(|_|"INVALID_GEOMETRY")?;
     let v=serde_json::json!({"type":"Feature","id":r.id,"geometry":ovg_spatial::to_geojson(&g.geometry),"properties":r.image.attributes});
     let b=serde_json::to_vec(&v).map_err(|_|"INVALID_GEOJSON")?;
     bytes=bytes.saturating_add(b.len()+1);if bytes>256*1024*1024{return Err("RESOURCE_LIMIT")}
     if count>0{f.write_all(b",").map_err(|_|"STORAGE_UNAVAILABLE")?;}
     f.write_all(&b).map_err(|_|"STORAGE_UNAVAILABLE")?;count+=1;
    }
    match page.next{Some(n) if n>after=>after=n,Some(_)=>return Err("INVALID_CURSOR"),None=>break}
   }
   f.write_all(b"]}").map_err(|_|"STORAGE_UNAVAILABLE")?;f.sync_all().map_err(|_|"STORAGE_UNAVAILABLE")?;drop(f);
   stage.persist_noclobber(path).map_err(|_|"DESTINATION_UNAVAILABLE")?;Ok(count)
  })
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
pub(crate) async fn work<T:Send+'static>(window:WebviewWindow,u:Unified,f:impl FnOnce(Unified)->UResult<T>+Send+'static)->Reply<T>{
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

#[cfg(test)]
mod tests{
 use super::*;
 #[test]fn new_project_atomic_multi_layer_and_stale_handle(){
  let d=tempfile::tempdir().unwrap();let u=Unified::new(d.path().to_path_buf()).unwrap();let p=u.create().unwrap();let w=u.open(p.id).unwrap();
  let c=u.with(w.handle,|s|s.catalog().map_err(err)).unwrap();let a=c.layers[0].id;let b=Uuid::new_v4();
  u.with(w.handle,|s|s.add_layer(b,"B",1).map_err(err)).unwrap();
  let x=Uuid::new_v4();let y=Uuid::new_v4();
  let edits=|revision:&str|vec![GeometryEdit{layer:a,feature:x,expected:"0".into(),geometry:Some(serde_json::json!({"type":"Point","coordinates":[106,11]})),attributes:serde_json::json!({})},GeometryEdit{layer:b,feature:y,expected:revision.into(),geometry:Some(serde_json::json!({"type":"Point","coordinates":[107,12]})),attributes:serde_json::json!({})}];
  assert!(u.apply(w.handle,Input{operation:Uuid::new_v4(),expected_project:"2".into(),edits:edits("7"),links:vec![]}).is_err());
  assert_eq!(u.with(w.handle,|s|s.revision().map_err(err)).unwrap(),2);
  u.apply(w.handle,Input{operation:Uuid::new_v4(),expected_project:"2".into(),edits:edits("0"),links:vec![]}).unwrap();
  u.close(w.handle).unwrap();assert!(u.with(w.handle,|s|s.catalog().map_err(err)).is_err());
  let w=u.open(p.id).unwrap();assert_eq!(u.with(w.handle,|s|s.feature(b,y,3).map_err(err)).unwrap().0,1);
  u.shutdown().unwrap();
 }
}

#[tauri::command]
pub async fn unified_backup(window:WebviewWindow,u:State<'_,Unified>,handle:Uuid,expected:String)->Reply<bool>{
 let app=window.app_handle().clone();
 work(window,u.inner().clone(),move|s|{
  let Some(file)=app.dialog().file().add_filter("Unified SQLite backup",&["sqlite"]).set_file_name("unified-backup.sqlite").blocking_save_file() else{return Ok(false)};
  let p=file.into_path().map_err(|_|"INVALID_STORAGE")?;
  if p.extension().and_then(|v|v.to_str())!=Some("sqlite"){return Err("UNSUPPORTED_FORMAT")}
  s.with(handle,|s|s.backup_copy(&p,number(&expected)?,&AtomicBool::new(false)).map_err(err))?;Ok(true)
 }).await
}
#[tauri::command]
pub async fn unified_restore(window:WebviewWindow,u:State<'_,Unified>)->Reply<Option<Project>>{
 let app=window.app_handle().clone();
 work(window,u.inner().clone(),move|s|{
  let Some(file)=app.dialog().file().add_filter("Unified SQLite backup or migration export",&["sqlite"]).blocking_pick_file() else{return Ok(None)};
  let p=file.into_path().map_err(|_|"INVALID_STORAGE")?;
  s.restore(&p).map(Some)
 }).await
}
#[tauri::command]
pub async fn unified_tile(window:WebviewWindow,u:State<'_,Unified>,handle:Uuid,layer:Uuid,expected:String,z:u8,x:u32,y:u32,request_id:Uuid)->Reply<tauri::ipc::Response>{
 trusted(&window)?;let read=ReadPermit::acquire(request_id)?;
 let slot=tokio::select!{
  p=READ_SLOTS.get_or_init(||tokio::sync::Semaphore::new(4)).acquire()=>p.map_err(|_|"SESSION_FAILED".to_owned())?,
  _=read.token.notify.notified()=>return Err("CANCELLED".into())
 };
 let b=work(window,u.inner().clone(),move|s|{
  let _slot=slot;
  if read.token.flag.load(Ordering::Acquire){return Err("CANCELLED")}
  let b=s.with(handle,|store|crate::mvt::unified_tile(store,layer,number(&expected)?,z,x,y,&read.token.flag))?;
  if read.token.flag.load(Ordering::Acquire){return Err("CANCELLED")}Ok(b)
 }).await?;
 Ok(tauri::ipc::Response::new(b))
}

#[cfg(test)]
mod backup_tests{
 use super::*;
 #[test]fn backup_restore_preserves_ids_revisions_and_mvt(){
  let d=tempfile::tempdir().unwrap();let u=Unified::new(d.path().join("projects")).unwrap();
  let p=u.create().unwrap();let w=u.open(p.id).unwrap();let catalog=u.with(w.handle,|s|s.catalog().map_err(err)).unwrap();
  let layer=catalog.layers[0].id;let id=Uuid::new_v4();let op=Uuid::new_v4();
  let input=||Input{operation:op,expected_project:"1".into(),edits:vec![GeometryEdit{layer,feature:id,expected:"0".into(),geometry:Some(serde_json::json!({"type":"Point","coordinates":[106.7,11.]})),attributes:serde_json::json!({"name":"survey"})}],links:vec![]};
  let receipt=u.apply(w.handle,input()).unwrap();
  let cancel=AtomicBool::new(false);let tile=u.with(w.handle,|s|crate::mvt::unified_tile(s,layer,2,0,0,0,&cancel)).unwrap();
  assert!(!tile.is_empty());assert!(u.with(w.handle,|s|crate::mvt::unified_tile(s,layer,1,0,0,0,&cancel)).is_err());
  let path=d.path().join("backup.sqlite");u.with(w.handle,|s|s.backup_copy(&path,2,&cancel).map_err(err)).unwrap();
  assert!(u.with(w.handle,|s|s.backup_copy(&path,2,&cancel).map_err(err)).is_err());
  let p2=u.restore(&path).unwrap();assert_ne!(p.id,p2.id);let w2=u.open(p2.id).unwrap();
  assert_eq!(u.apply(w2.handle,input()).unwrap(),receipt);
  assert_eq!(u.with(w2.handle,|s|crate::mvt::unified_tile(s,layer,2,0,0,0,&cancel)).unwrap(),tile);
  assert_eq!(u.with(w2.handle,|s|s.feature(layer,id,2).map_err(err)).unwrap().1.attributes["name"],"survey");
  u.with(w2.handle,|s|s.compensate_latest(Uuid::new_v4(),2).map_err(err)).unwrap();
  assert!(u.with(w2.handle,|s|s.feature(layer,id,3).map_err(err)).is_err());
  assert!(u.with(w.handle,|s|s.feature(layer,id,2).map_err(err)).is_ok());
  assert!(u.with(w2.handle,|s|crate::mvt::unified_tile(s,layer,3,0,0,0,&AtomicBool::new(true))).is_err());
  u.shutdown().unwrap();
 }
}

#[tauri::command]
pub async fn unified_import_geojson(window:WebviewWindow,u:State<'_,Unified>,handle:Uuid,layer:Uuid,expected:String)->Reply<Option<Value>>{
 let app=window.app_handle().clone();
 work(window,u.inner().clone(),move|s|{
  let Some(file)=app.dialog().file().add_filter("WGS84 GeoJSON (max 64 features)",&["geojson","json"]).blocking_pick_file() else{return Ok(None)};
  let path=file.into_path().map_err(|_|"INVALID_STORAGE")?;
  s.import_geojson(handle,layer,&path,expected,Uuid::new_v4()).map(Some)
 }).await
}
#[tauri::command]
pub async fn unified_export_geojson(window:WebviewWindow,u:State<'_,Unified>,handle:Uuid,layer:Uuid,expected:String)->Reply<Option<u64>>{
 let app=window.app_handle().clone();
 work(window,u.inner().clone(),move|s|{
  let Some(file)=app.dialog().file().add_filter("WGS84 GeoJSON",&["geojson"]).set_file_name("layer.geojson").blocking_save_file() else{return Ok(None)};
  let path=file.into_path().map_err(|_|"INVALID_STORAGE")?;
  if path.extension().and_then(|x|x.to_str())!=Some("geojson"){return Err("UNSUPPORTED_FORMAT")}
  s.export_geojson(handle,layer,&path,number(&expected)?).map(Some)
 }).await
}

#[cfg(test)]
mod interchange_tests{
 use super::*;
 #[test]fn geojson_atomic_import_export_and_no_clobber(){
  let d=tempfile::tempdir().unwrap();let u=Unified::new(d.path().join("projects")).unwrap();let p=u.create().unwrap();let w=u.open(p.id).unwrap();
  let l=u.with(w.handle,|s|s.catalog().map_err(err)).unwrap().layers[0].id;
  let input=d.path().join("input.geojson");
  let feature=serde_json::json!({"type":"Feature","geometry":{"type":"Point","coordinates":[106,11]},"properties":{"name":"A"}});
  let invalid=serde_json::json!({"type":"FeatureCollection","features":[feature.clone(),{"type":"Feature","geometry":{"type":"Point","coordinates":[106,11,5]},"properties":{}}]});
  fs::write(&input,invalid.to_string()).unwrap();
  assert!(u.import_geojson(w.handle,l,&input,"1".into(),Uuid::new_v4()).is_err());
  assert_eq!(u.with(w.handle,|s|s.revision().map_err(err)).unwrap(),1);
  fs::write(&input,serde_json::json!({"type":"FeatureCollection","features":[feature]}).to_string()).unwrap();
  u.import_geojson(w.handle,l,&input,"1".into(),Uuid::new_v4()).unwrap();
  let dest=d.path().join("out.geojson");assert_eq!(u.export_geojson(w.handle,l,&dest,2).unwrap(),1);
  let v:Value=serde_json::from_slice(&fs::read(&dest).unwrap()).unwrap();assert_eq!(v["features"][0]["properties"]["name"],"A");
  assert!(u.export_geojson(w.handle,l,&dest,2).is_err());assert!(u.export_geojson(w.handle,l,&d.path().join("stale.geojson"),1).is_err());
  u.shutdown().unwrap();
 }
}
