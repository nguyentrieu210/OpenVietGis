//! Native host boundary for the same unified SQLite kernel on Desktop and Mobile.
//! Trusted application chooses the private root. This is standalone data only.
use super::*;
use std::{fs,path::PathBuf,sync::atomic::AtomicBool};
const MAX_PROJECTS:usize=64;
const MAX_JSON:usize=4*1024*1024;
fn root(p:&Path)->Result<PathBuf>{
 if !p.is_absolute(){return Err(Error::Invalid)}
 fs::create_dir_all(p).map_err(|_|Error::Storage)?;
 if !fs::symlink_metadata(p).map_err(|_|Error::Storage)?.is_dir(){return Err(Error::Invalid)}
 p.canonicalize().map_err(|_|Error::Storage)
}
fn id(s:&str)->Result<Uuid>{
 let id=Uuid::parse_str(s).map_err(|_|Error::Invalid)?;
 if id.is_nil()||id.to_string()!=s{return Err(Error::Invalid)}Ok(id)
}
fn encode<T:Serialize>(v:&T)->Result<String>{
 let s=json(v)?;if s.len()>MAX_JSON{return Err(Error::Limit)}Ok(s)
}
fn revision(s:&str)->Result<u64>{
 let n=s.parse::<u64>().map_err(|_|Error::Invalid)?;
 if n.to_string()!=s||n>i64::MAX as u64{return Err(Error::Invalid)}Ok(n)
}
pub fn list_projects(path:&Path)->Result<Vec<String>>{
 let mut out=Vec::new();
 for ent in fs::read_dir(root(path)?).map_err(|_|Error::Storage)?{
  let ent=ent.map_err(|_|Error::Storage)?;
  let name=ent.file_name().to_string_lossy().into_owned();
  if id(&name).is_ok()&&ent.file_type().map_err(|_|Error::Storage)?.is_dir()&&
     fs::symlink_metadata(ent.path().join("project.sqlite")).is_ok_and(|m|m.is_file()){
   out.push(name);if out.len()>MAX_PROJECTS{return Err(Error::Limit)}
  }
 }out.sort();Ok(out)
}
pub struct NativeProject{store:Mutex<Option<Store>>,id:Uuid}
impl NativeProject{
 fn with<T>(&self,f:impl FnOnce(&Store)->Result<T>)->Result<T>{
  let mut s=self.store.lock().map_err(lock_err)?;
  let store=s.as_ref().ok_or(Error::Conflict)?;
  match std::panic::catch_unwind(std::panic::AssertUnwindSafe(||f(store))){
   Ok(v)=>v,Err(_)=>{s.take();Err(Error::Storage)}
  }
 }
 pub fn project_id(&self)->String{self.id.to_string()}
 pub fn close(&self)->Result<()>{let mut s=self.store.lock().map_err(lock_err)?;if let Some(store)=s.as_ref(){store.checkpoint()?;}s.take();Ok(())}
 pub fn catalog(&self)->Result<String>{self.with(|s|encode(&s.catalog()?))}
 pub fn add_layer(&self,layer:&str,name:&str,expected:&str)->Result<String>{
  self.with(|s|Ok(s.add_layer(id(layer)?,name,revision(expected)?)?.to_string()))
 }
 pub fn apply(&self,batch_json:&str)->Result<String>{
  if batch_json.len()>MAX_JSON{return Err(Error::Limit)}
  let b:Batch=decode(batch_json)?;
  self.with(|s|{let r=s.apply(&b)?;encode(&serde_json::json!({"revision":r.revision.to_string(),"features":r.features.into_iter().map(|(l,f,r)|(l,f,r.to_string())).collect::<Vec<_>>() }))})
 }
 pub fn compensate(&self,operation:&str,expected:&str)->Result<String>{
  self.with(|s|{let r=s.compensate_latest(id(operation)?,revision(expected)?)?;encode(&serde_json::json!({"revision":r.revision.to_string(),"features":r.features.into_iter().map(|(l,f,r)|(l,f,r.to_string())).collect::<Vec<_>>() }))})
 }
 pub fn query(&self,layer:&str,bbox:[f64;4],after:&str,limit:u32,expected:&str)->Result<String>{
  let cursor=after.parse::<i64>().map_err(|_|Error::Invalid)?;
  if cursor<0||cursor.to_string()!=after{return Err(Error::Invalid)}
  self.with(|s|{
   // Kernel returns bounded WKB; JSON arrays can expand. Limit wire bytes too.
   let p=s.query(id(layer)?,bbox,cursor,limit,revision(expected)?)?;
   let mut rows=Vec::new();let mut bytes=0usize;let mut next=p.next;
   for r in p.rows{
    let v=serde_json::json!({"fid":r.fid.to_string(),"id":r.id,"revision":r.revision.to_string(),"image":r.image});
    let n=json(&v)?.len();
    if bytes+n>MAX_JSON-1024{
     if rows.is_empty(){return Err(Error::Limit)}
     next=rows.last().and_then(|x:&Value|x["fid"].as_str()?.parse().ok());break
    }
    bytes+=n+1;rows.push(v);
   }
   encode(&serde_json::json!({"revision":p.revision.to_string(),"rows":rows,"next":next.map(|n|n.to_string())}))
  })
 }
 pub fn integrity(&self)->Result<bool>{self.with(|s|s.integrity())}
 /// Backup into private exchange directory, never caller path or cloud.
 pub fn backup(&self,exchange:&Path,expected:&str)->Result<String>{
  let name=format!("{}.sqlite",Uuid::new_v4());let dest=root(exchange)?.join(&name);
  self.with(|s|s.backup_copy(&dest,revision(expected)?,&AtomicBool::new(false)))?;Ok(name)
 }
}
pub fn open_project(path:&Path,project:&str)->Result<NativeProject>{
 let id=id(project)?;let dir=root(path)?.join(project);
 if !fs::symlink_metadata(&dir).is_ok_and(|m|m.is_dir()){return Err(Error::Invalid)}
 let store=Store::reopen(&dir.join("project.sqlite"))?;
 Ok(NativeProject{store:Mutex::new(Some(store)),id})
}
fn publish(path:&Path,f:impl FnOnce(&Path)->Result<()>)->Result<String>{
 let root=root(path)?;if list_projects(&root)?.len()>=MAX_PROJECTS{return Err(Error::Limit)}
 let project=Uuid::new_v4();let stage=root.join(format!(".staging-{project}"));let dst=root.join(project.to_string());
 fs::create_dir(&stage).map_err(|_|Error::Storage)?;
 let result=(||{
  f(&stage.join("project.sqlite"))?;
  if dst.exists(){return Err(Error::Conflict)}
  fs::rename(&stage,&dst).map_err(|_|Error::Storage)?;
  #[cfg(unix)]{fs::File::open(&root).and_then(|f|f.sync_all()).map_err(|_|Error::Storage)?;}
  Ok(project.to_string())
 })();
 if result.is_err(){let _=fs::remove_dir_all(&stage);}result
}
pub fn create_project(path:&Path)->Result<String>{
 publish(path,|p|{let s=Store::create(p)?;s.add_layer(Uuid::new_v4(),"Layer 1",0)?;s.checkpoint()})
}
/// Import a checkpointed standalone backup into a new project. A separate
/// exchange root is supplied by the trusted app; filename must be UUID.sqlite.
pub fn restore_project(path:&Path,exchange:&Path,name:&str)->Result<String>{
 let stem=name.strip_suffix(".sqlite").ok_or(Error::Invalid)?;id(stem)?;
 let source=root(exchange)?.join(name);
 publish(path,|p|{Store::restore_copy(&source,p,&AtomicBool::new(false))?;Ok(())})
}
#[cfg(test)]mod tests{
 use super::*;
 #[test]fn mobile_host_roundtrip_same_kernel(){
  let d=tempfile::tempdir().unwrap();let root=d.path().join("unified");let ex=d.path().join("exchange");
  let p=create_project(&root).unwrap();let w=open_project(&root,&p).unwrap();assert_eq!(list_projects(&root).unwrap(),vec![p.clone()]);
  let c:Value=decode(&w.catalog().unwrap()).unwrap();let layer=c["layers"][0]["id"].as_str().unwrap();
  let feature=Uuid::new_v4();let op=Uuid::new_v4();let b=Batch{operation:op,expected_project:1,edits:vec![Edit{layer:id(layer).unwrap(),feature,expected:0,image:Some(Image{wkb:ovg_mobile_engine::edit::draft("point",&[106.,11.]).unwrap(),attributes:serde_json::json!({"name":"mobile"})})}],links:vec![]};
  let text=json(&b).unwrap();let receipt=w.apply(&text).unwrap();assert_eq!(w.apply(&text).unwrap(),receipt);
  let copy=w.backup(&ex,"2").unwrap();w.close().unwrap();assert!(w.catalog().is_err());
  let restored=restore_project(&root,&ex,&copy).unwrap();let r=open_project(&root,&restored).unwrap();assert_eq!(r.apply(&text).unwrap(),receipt);
  let rows:Value=decode(&r.query(layer,[105.,10.,107.,12.],"0",20,"2").unwrap()).unwrap();
  assert_eq!(rows["rows"][0]["id"],feature.to_string());assert!(r.integrity().unwrap());
  assert!(restore_project(&root,&ex,"../wrong.sqlite").is_err());
 }
}
