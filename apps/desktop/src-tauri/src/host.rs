use ovg_local_storage::LocalStore;
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
struct Open { engine: Workspace, _store: LocalStore, project: Uuid }
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
impl Host {
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
        s.open.insert(handle,Open{engine:workspace,_store:store,project:id});
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
