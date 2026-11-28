use ovg_local_storage::{LocalStore, editing::Edit, Error as StoreError};
use std::io::{Read, Write};
use serde_json::{Value,json};
use ovg_mobile_engine::{Engine, Job, Session, Workspace};
use serde::Serialize;
use std::{collections::HashMap, fs, path::{Path, PathBuf}, sync::{Arc, Mutex}};
use uuid::Uuid;

type Result<T> = std::result::Result<T, &'static str>;
const QUOTA: u64 = 512 * 1024 * 1024;
const MAX_PROJECTS: usize = 64;
const MAX_OPEN: usize = 8;
const MAX_JOBS: usize = 16;

#[derive(Clone, Serialize)]
pub struct Project { pub id: Uuid }
#[derive(Serialize)]
pub struct Opened { pub id: Uuid, pub handle: Uuid }
#[derive(Serialize)]
#[serde(tag="status", rename_all="snake_case")]
pub enum Poll { Pending, Complete { xy: Vec<f64> } }
struct Open { engine: Workspace, _store: LocalStore, project: Uuid, generation: u64, raster: Option<(Uuid,ovg_local_storage::mbtiles::Archive)> }
struct TrackedJob { handle: Uuid, job: Job }
struct State { session: Session, open: HashMap<Uuid,Open>, jobs: HashMap<Uuid,TrackedJob> }
#[derive(Clone)]
pub struct Host { root: PathBuf, engine: Engine, state: Arc<Mutex<State>> }

fn real_dir(p: &Path) -> Result<()> {
    let m = fs::symlink_metadata(p).map_err(|_|"STORAGE_UNAVAILABLE")?;
    if !m.file_type().is_dir() { return Err("INVALID_STORAGE"); }
    Ok(())
}
fn regular(p: &Path) -> Result<()> {
    if !fs::symlink_metadata(p).map_err(|_|"PROJECT_NOT_FOUND")?.file_type().is_file() {
        return Err("INVALID_STORAGE");
    }
    Ok(())
}
fn err(e: ovg_mobile_engine::Error) -> &'static str {
    use ovg_mobile_engine::Error::*;
    match e { Stale => "STALE_HANDLE", Limit => "RESOURCE_LIMIT", InvalidBatch => "INVALID_BATCH",
        Cancelled => "CANCELLED", Failed => "SESSION_FAILED", Spatial(_) => "SPATIAL_ERROR", Pending => "JOB_PENDING" }
}
fn store_err(e:StoreError)->&'static str {
    match e {StoreError::Conflict=>"REVISION_CONFLICT",StoreError::Reused=>"OPERATION_ID_REUSED",
      StoreError::Invalid=>"INVALID_LOCAL_DATA",StoreError::Limit=>"RESOURCE_LIMIT",
      StoreError::Unsupported=>"UNSUPPORTED_DATABASE",_=>"STORAGE_UNAVAILABLE"}
}
const MAX_IMPORT:u64=16*1024*1024;
const MAX_READ:usize=2*1024*1024;
#[derive(Serialize)]
pub struct Viewport { pub generation:String, pub data:Value, pub truncated:bool }
#[derive(Serialize)]
pub struct Raster { pub id:Uuid, pub attribution:String }
impl Host {
    fn read<T>(&self,handle:Uuid,f:impl FnOnce(&Open)->Result<T>)->Result<T>{
        let s=self.lock()?; f(s.open.get(&handle).ok_or("STALE_HANDLE")?)
    }
    fn mutate<T>(&self,handle:Uuid,f:impl FnOnce(&LocalStore)->Result<T>)->Result<T>{
        let mut s=self.lock()?;let w=s.open.get_mut(&handle).ok_or("STALE_HANDLE")?;
        let next=w.generation.checked_add(1).ok_or("RESOURCE_LIMIT")?;
        let out=f(&w._store)?;w.generation=next;Ok(out)
    }
    // Session-scoped, explicitly selected raster archive. No persistent path grants.
    pub fn open_raster(&self,handle:Uuid,path:&Path)->Result<Raster>{
        regular(path)?;
        let archive=ovg_local_storage::mbtiles::Archive::open(path).map_err(store_err)?;
        if archive.format!="png"{return Err("UNSUPPORTED_FORMAT")}
        let id=Uuid::new_v4();let attribution=archive.attribution.clone();
        let mut s=self.lock()?;let w=s.open.get_mut(&handle).ok_or("STALE_HANDLE")?;
        w.raster=Some((id,archive));Ok(Raster{id,attribution})
    }
    pub fn raster_tile(&self,handle:Uuid,id:Uuid,z:u8,x:u32,y:u32)->Result<Vec<u8>>{
        if z>22||x>=(1u32<<z)||y>=(1u32<<z){return Err("INVALID_TILE")}
        self.read(handle,|w|{
            let (_,a)=w.raster.as_ref().filter(|(key,_)|*key==id).ok_or("STALE_HANDLE")?;
            let b=a.tile(z,x,y).map_err(store_err)?.ok_or("TILE_NOT_FOUND")?;
            if b.len()<33||b.len()>4*1024*1024||&b[..8]!=b"\\x89PNG\\r\\n\\x1a\\n"||&b[12..16]!=b"IHDR"{return Err("INVALID_TILE")}
            let width=u32::from_be_bytes(b[16..20].try_into().map_err(|_|"INVALID_TILE")?);
            let height=u32::from_be_bytes(b[20..24].try_into().map_err(|_|"INVALID_TILE")?);
            if !matches!(width,256|512)||height!=width{return Err("INVALID_TILE")}
            Ok(b)
        })
    }
    pub fn close_raster(&self,handle:Uuid)->Result<()>{
        let mut s=self.lock()?;let w=s.open.get_mut(&handle).ok_or("STALE_HANDLE")?;
        w.raster=None;Ok(())
    }
    pub fn generation(&self,handle:Uuid)->Result<String>{self.read(handle,|w|Ok(w.generation.to_string()))}
    pub fn viewport(&self,handle:Uuid,bbox:[f64;4])->Result<Viewport>{
        if bbox.iter().any(|x|!x.is_finite())||bbox[0]< -180.||bbox[2]>180.||bbox[1]< -90.||bbox[3]>90.||bbox[0]>bbox[2]||bbox[1]>bbox[3]{return Err("INVALID_BBOX")}
        self.read(handle,|w|{
            let raw=w._store.viewport_geojson(bbox).map_err(store_err)?;
            if raw.len()>MAX_READ{return Err("RESOURCE_LIMIT")}
            let v:Value=serde_json::from_str(&raw).map_err(|_|"INVALID_LOCAL_DATA")?;
            Ok(Viewport{generation:w.generation.to_string(),data:v["data"].clone(),truncated:v["truncated"].as_bool().ok_or("INVALID_LOCAL_DATA")?})
        })
    }
    pub fn attributes(&self,handle:Uuid,after:String,limit:u32,contains:String)->Result<Value>{
        let n=after.parse::<i64>().map_err(|_|"INVALID_CURSOR")?;
        self.read(handle,|w|{
            let raw=w._store.attribute_page(n,limit,&contains).map_err(store_err)?;
            if raw.len()>MAX_READ{return Err("RESOURCE_LIMIT")}
            let mut v:Value=serde_json::from_str(&raw).map_err(|_|"INVALID_LOCAL_DATA")?;
            for row in v.as_array_mut().ok_or("INVALID_LOCAL_DATA")?{
                for field in ["fid","revision"]{let val=row[field].as_u64().ok_or("INVALID_LOCAL_DATA")?;row[field]=json!(val.to_string());}
            }
            Ok(json!({"generation":w.generation.to_string(),"rows":v}))
        })
    }
    pub fn feature(&self,handle:Uuid,id:Uuid)->Result<Value>{
        self.read(handle,|w|{
            let raw=w._store.feature_json(id).map_err(store_err)?;
            if raw.len()>MAX_READ{return Err("RESOURCE_LIMIT")}
            let mut v:Value=serde_json::from_str(&raw).map_err(|_|"INVALID_LOCAL_DATA")?;
            let rev=v["revision"].as_u64().ok_or("INVALID_LOCAL_DATA")?;
            v["revision"]=json!(rev.to_string());Ok(v)
        })
    }
    pub fn edit(&self,handle:Uuid,command:Uuid,id:Uuid,revision:String,kind:Option<String>,xy:Vec<f64>,attrs:Value)->Result<()>{
        let expected_revision=revision.parse::<u64>().map_err(|_|"INVALID_REVISION")?;
        if !attrs.is_object()||attrs.to_string().len()>256*1024||xy.len()>8192{return Err("RESOURCE_LIMIT")}
        let wkb=match kind {Some(k)=>Some(ovg_mobile_engine::edit::draft(&k,&xy).map_err(err)?),None if xy.is_empty()=>None,_=>return Err("INVALID_BATCH")};
        self.mutate(handle,|s|s.commit_edit(command,&[Edit{feature_id:id,expected_revision,wkb,attributes:attrs}]).map_err(store_err))
    }
    pub fn update_attributes(&self,handle:Uuid,id:Uuid,revision:String,attrs:Value)->Result<()>{
        if !attrs.is_object()||attrs.to_string().len()>256*1024{return Err("RESOURCE_LIMIT")}
        let rev=revision.parse().map_err(|_|"INVALID_REVISION")?;
        self.mutate(handle,|s|s.update_attributes(id,rev,attrs).map_err(store_err))
    }
    pub fn history(&self,handle:Uuid,redo:bool)->Result<bool>{
        self.mutate(handle,|s|s.history_step(redo).map_err(store_err))
    }
    // Path is obtained exclusively from the native dialog; never from renderer IPC.
    pub fn import_geojson(&self,handle:Uuid,path:&Path)->Result<u32>{
        regular(path)?;
        if fs::metadata(path).map_err(|_|"STORAGE_UNAVAILABLE")?.len()>MAX_IMPORT{return Err("RESOURCE_LIMIT")}
        let mut buf=Vec::new();
        fs::File::open(path).map_err(|_|"STORAGE_UNAVAILABLE")?.take(MAX_IMPORT+1).read_to_end(&mut buf).map_err(|_|"STORAGE_UNAVAILABLE")?;
        if buf.len() as u64>MAX_IMPORT{return Err("RESOURCE_LIMIT")}
        self.mutate(handle,|s|s.import_geojson(&buf).map_err(store_err))
    }
    pub fn snapshot(&self,handle:Uuid,path:&Path)->Result<()>{
        if path.exists(){return Err("DESTINATION_EXISTS")}
        self.read(handle,|w|w._store.export_snapshot(path).map_err(store_err))
    }
    pub fn export_geojson(&self,handle:Uuid,path:&Path)->Result<()>{
        self.read(handle,|w|{
            let data=w._store.export_geojson().map_err(store_err)?;
            let mut f=fs::OpenOptions::new().write(true).create_new(true).open(path).map_err(|_|"DESTINATION_UNAVAILABLE")?;
            if f.write_all(data.as_bytes()).and_then(|_|f.sync_all()).is_err(){drop(f);let _=fs::remove_file(path);return Err("STORAGE_UNAVAILABLE")}
            Ok(())
        })
    }
    pub fn checkpoint(&self,handle:Uuid)->Result<()>{
        self.read(handle,|w|w._store.checkpoint().map_err(store_err))
    }

    pub fn new(root: PathBuf) -> Result<Self> {
        fs::create_dir_all(&root).map_err(|_|"STORAGE_UNAVAILABLE")?;
        real_dir(&root)?;
        let engine=Engine::default();
        let session=engine.open_session().map_err(err)?;
        Ok(Self{root,engine,state:Arc::new(Mutex::new(State{session,open:HashMap::new(),jobs:HashMap::new()}))})
    }
    fn lock(&self) -> Result<std::sync::MutexGuard<'_,State>> {
        self.state.lock().map_err(|_|"SESSION_FAILED")
    }
    fn project_path(&self,id:Uuid)->PathBuf { self.root.join(id.to_string()).join("workspace.gpkg") }
    pub fn shutdown(&self)->Result<()> {
        let mut s=self.lock()?;
        self.engine.close_session(s.session).map_err(err)?;
        s.jobs.clear();s.open.clear();Ok(())
    }
    pub fn list(&self)->Result<Vec<Project>> {
        real_dir(&self.root)?;
        let mut out=Vec::new();
        for item in fs::read_dir(&self.root).map_err(|_|"STORAGE_UNAVAILABLE")? {
            let item=item.map_err(|_|"STORAGE_UNAVAILABLE")?;
            let Some(name)=item.file_name().to_str().map(str::to_owned) else {continue};
            let Ok(id)=Uuid::parse_str(&name) else {continue};
            if name != id.to_string() || real_dir(&item.path()).is_err() {continue}
            if regular(&self.project_path(id)).is_ok() {
                if out.len()>=MAX_PROJECTS {return Err("RESOURCE_LIMIT")}
                out.push(Project{id});
            }
        }
        out.sort_by_key(|p|p.id);
        Ok(out)
    }
    pub fn create(&self)->Result<Project> {
        let _guard=self.lock()?;
        if self.list()?.len()>=MAX_PROJECTS{return Err("RESOURCE_LIMIT")}
        let id=Uuid::new_v4();
        let dir=self.root.join(id.to_string());
        fs::create_dir(&dir).map_err(|_|"STORAGE_UNAVAILABLE")?;
        let path=self.project_path(id);
        match LocalStore::open(&path,QUOTA) {
            Ok(store)=>{store.checkpoint().map_err(|_|"STORAGE_UNAVAILABLE")?;Ok(Project{id})},
            Err(_)=>Err("STORAGE_UNAVAILABLE"),
        }
    }
    pub fn open(&self,id:Uuid)->Result<Opened> {
        let mut s=self.lock()?;
        if s.open.len()>=MAX_OPEN{return Err("RESOURCE_LIMIT")}
        if s.open.values().any(|v|v.project==id){return Err("PROJECT_ALREADY_OPEN")}
        real_dir(&self.root)?;
        real_dir(&self.root.join(id.to_string()))?;
        let path=self.project_path(id);
        regular(&path)?;
        for suffix in ["-wal","-shm","-journal"] {
            let side=path.with_file_name(format!("workspace.gpkg{suffix}"));
            match fs::symlink_metadata(side) {
                Ok(m) if !m.file_type().is_file()=>return Err("INVALID_STORAGE"),
                Err(e) if e.kind()!=std::io::ErrorKind::NotFound=>return Err("INVALID_STORAGE"),
                _=>{}
            }
        }
        let store=LocalStore::open(&path,QUOTA).map_err(|_|"STORAGE_UNAVAILABLE")?;
        let workspace=self.engine.open_workspace(s.session).map_err(err)?;
        let handle=Uuid::new_v4();
        s.open.insert(handle,Open{engine:workspace,_store:store,project:id,generation:0,raster:None});
        Ok(Opened{id,handle})
    }
    pub fn close(&self,handle:Uuid)->Result<()> {
        let mut s=self.lock()?;
        let open=s.open.get(&handle).ok_or("STALE_HANDLE")?;
        open._store.checkpoint().map_err(|_|"STORAGE_UNAVAILABLE")?;
        self.engine.close_workspace(open.engine).map_err(err)?;
        s.open.remove(&handle);
        s.jobs.retain(|_,j|j.handle!=handle);
        Ok(())
    }
    pub fn start(&self,handle:Uuid,source:u32,target:u32,xy:Vec<f64>)->Result<Uuid>{
        if xy.is_empty()||xy.len()>8192||xy.len()%2!=0||xy.iter().any(|v|!v.is_finite()){return Err("INVALID_BATCH")}
        let mut s=self.lock()?;
        if s.jobs.len()>=MAX_JOBS{return Err("RESOURCE_LIMIT")}
        let w=s.open.get(&handle).ok_or("STALE_HANDLE")?.engine;
        let job=self.engine.start_transform(w,source,target,xy).map_err(err)?;
        let id=Uuid::new_v4();
        s.jobs.insert(id,TrackedJob{handle,job});Ok(id)
    }
    pub fn poll(&self,handle:Uuid,id:Uuid)->Result<Poll>{
        let mut s=self.lock()?;
        let j=s.jobs.get(&id).filter(|j|j.handle==handle).ok_or("STALE_HANDLE")?.job;
        match self.engine.is_finished(j) {
            Ok(false)=>Ok(Poll::Pending),
            Ok(true)=>{let result=self.engine.take_result(j);s.jobs.remove(&id);result.map(|xy|Poll::Complete{xy}).map_err(err)},
            Err(e)=>{s.jobs.remove(&id);Err(err(e))}
        }
    }
    pub fn cancel(&self,handle:Uuid,id:Uuid)->Result<()>{
        let mut s=self.lock()?;
        let j=s.jobs.get(&id).filter(|j|j.handle==handle).ok_or("STALE_HANDLE")?.job;
        self.engine.dispose_job(j).map_err(err)?;
        s.jobs.remove(&id);Ok(())
    }
}
#[cfg(test)]
mod tests{
    use super::*;
    #[test] fn local_data_edit_undo_and_restart(){
        let root=std::env::temp_dir().join(format!("ovg-w2-{}",Uuid::new_v4()));
        let h=Host::new(root.clone()).unwrap();let p=h.create().unwrap();let w=h.open(p.id).unwrap();
        let id=Uuid::new_v4();
        h.edit(w.handle,Uuid::new_v4(),id,"0".into(),Some("point".into()),vec![106.7,11.],json!({"name":"test"})).unwrap();
        let v=h.viewport(w.handle,[106.,10.,108.,12.]).unwrap();assert_eq!(v.data["features"].as_array().unwrap().len(),1);
        assert!(h.viewport(w.handle,[0.,0.,1.,1.]).unwrap().data["features"].as_array().unwrap().is_empty());
        let f=h.feature(w.handle,id).unwrap();let rev=f["revision"].as_str().unwrap().to_owned();
        assert!(h.update_attributes(w.handle,id,"0".into(),json!({})).is_err());
        h.update_attributes(w.handle,id,rev,json!({"name":"changed"})).unwrap();
        assert!(h.history(w.handle,false).unwrap());assert_eq!(h.feature(w.handle,id).unwrap()["attributes"]["name"],"test");
        assert!(h.history(w.handle,true).unwrap());
        let backup=root.join("backup.gpkg");h.snapshot(w.handle,&backup).unwrap();assert!(h.snapshot(w.handle,&backup).is_err());
        h.close(w.handle).unwrap();let w=h.open(p.id).unwrap();
        assert_eq!(h.feature(w.handle,id).unwrap()["attributes"]["name"],"changed");
        assert!(h.attributes(w.handle,"0".into(),200,"".into()).unwrap()["rows"].as_array().unwrap().len()==1);
        h.close(w.handle).unwrap();drop(h);fs::remove_dir_all(root).unwrap();
    }
    #[test] fn local_lifecycle_and_real_transform(){
        let root=std::env::temp_dir().join(format!("ovg-w1-{}",Uuid::new_v4()));
        let h=Host::new(root.clone()).unwrap();
        let p=h.create().unwrap();
        assert_eq!(h.list().unwrap().len(),1);
        let w=h.open(p.id).unwrap();
        assert!(h.open(p.id).is_err());
        let j=h.start(w.handle,4326,3857,vec![1.,0.]).unwrap();
        assert!(h.poll(Uuid::new_v4(),j).is_err());
        let deadline=std::time::Instant::now()+std::time::Duration::from_secs(5);
        loop {match h.poll(w.handle,j).unwrap(){
            Poll::Pending=>{assert!(std::time::Instant::now()<deadline);std::thread::sleep(std::time::Duration::from_millis(5));},
            Poll::Complete{xy}=>{assert!((xy[0]-111319.49).abs()<0.01);break;}
        }}
        assert!(h.poll(w.handle,j).is_err());
        h.close(w.handle).unwrap();
        assert!(h.start(w.handle,4326,3857,vec![1.,0.]).is_err());
        let w2=h.open(p.id).unwrap();h.close(w2.handle).unwrap();
        drop(h);fs::remove_dir_all(root).unwrap();
    }
}
