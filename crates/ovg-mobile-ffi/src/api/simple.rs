//! Mobile host bridge. The private directory is supplied only by the trusted app,
//! never by a downloaded plugin. No managed data or credentials are accepted.
use ovg_local_storage::{LocalStore, Mutation};
use ovg_mobile_engine::{Engine, Session, Workspace, Job};
use std::{fs, path::{Path, PathBuf}, sync::Mutex};
use uuid::Uuid;

const QUOTA: u64 = 512 * 1024 * 1024;
const MAX_LOCAL_WORKSPACES: usize = 128;
#[flutter_rust_bridge::frb(init)]
pub fn init_app() { flutter_rust_bridge::setup_default_user_utils(); }

fn root(path: &str)->Result<PathBuf,String>{
    let p=Path::new(path);
    if !p.is_absolute() {return Err("INVALID_PRIVATE_ROOT".into());}
    fs::create_dir_all(p).map_err(|_|"PRIVATE_ROOT_UNAVAILABLE".to_owned())?;
    if !fs::symlink_metadata(p).map_err(|_|"PRIVATE_ROOT_UNAVAILABLE".to_owned())?.is_dir(){return Err("INVALID_PRIVATE_ROOT".into());}
    p.canonicalize().map_err(|_|"PRIVATE_ROOT_UNAVAILABLE".into())
}
fn id(value:&str)->Result<Uuid,String>{Uuid::parse_str(value).ok().filter(|id|!id.is_nil()&&id.to_string()==value).ok_or_else(||"INVALID_WORKSPACE_ID".into())}
pub fn list_local_workspaces(root_path:String)->Result<Vec<String>,String>{
    let mut ids=Vec::new();
    for entry in fs::read_dir(root(&root_path)?).map_err(|_|"PRIVATE_ROOT_UNAVAILABLE".to_owned())? {
        let entry=entry.map_err(|_|"PRIVATE_ROOT_UNAVAILABLE".to_owned())?;
        let name=entry.file_name().to_string_lossy().into_owned();
        if id(&name).is_ok() && entry.file_type().map_err(|_|"PRIVATE_ROOT_UNAVAILABLE".to_owned())?.is_dir()
            && fs::symlink_metadata(entry.path().join("workspace.gpkg")).is_ok_and(|m|m.file_type().is_file()){
            if ids.len()>=MAX_LOCAL_WORKSPACES{return Err("WORKSPACE_LIMIT".into());}
            ids.push(name);
        }
    }
    ids.sort();Ok(ids)
}
pub fn create_local_workspace(root_path:String)->Result<String,String>{
    if list_local_workspaces(root_path.clone())?.len()>=MAX_LOCAL_WORKSPACES{return Err("WORKSPACE_LIMIT".into());}
    let workspace_id=Uuid::new_v4().to_string();
    let dir=root(&root_path)?.join(&workspace_id);
    fs::create_dir(&dir).map_err(|_|"CREATE_WORKSPACE_FAILED".to_owned())?;
    match LocalStore::open(&dir.join("workspace.gpkg"),QUOTA){
        Ok(store)=>{drop(store);Ok(workspace_id)}
        Err(e)=>{let _=fs::remove_dir_all(&dir);Err(e.to_string())}
    }
}
/// Publish a completed standalone WGS84 result to a new private workspace.
/// Projected results remain explicit CRS-tagged WKB export, not mislabeled GPKG.
pub fn publish_processing_result(root_path:String,result_json:String)->Result<String,String>{
    if result_json.len()>20*1024*1024{return Err("RESOURCE_LIMIT".into());}
    let r:ovg_mobile_engine::processing::ResultSet=serde_json::from_str(&result_json).map_err(|_|"INVALID_RESULT".to_owned())?;
    if r.epsg!=4326||r.rows.len()>4096||r.adapter!="geo-0.31-mobile-bounded-v1"{return Err("UNSUPPORTED_OUTPUT_CRS_OR_PROFILE".into());}
    if list_local_workspaces(root_path.clone())?.len()>=MAX_LOCAL_WORKSPACES{return Err("WORKSPACE_LIMIT".into());}
    let root=root(&root_path)?;let id=Uuid::new_v4().to_string();let stage=root.join(format!(".processing-{id}"));let dst=root.join(&id);
    fs::create_dir(&stage).map_err(|_|"CREATE_WORKSPACE_FAILED".to_owned())?;
    let result=(||->Result<(),String>{
        let store=LocalStore::open(&stage.join("workspace.gpkg"),QUOTA).map_err(|e|e.to_string())?;
        let mut mutations=Vec::new();
        for row in r.rows{
            ovg_mobile_engine::edit::validate(&row.wkb).map_err(|e|e.to_string())?;
            mutations.push(Mutation{operation_id:Uuid::new_v4(),feature_id:Uuid::new_v4(),expected_revision:0,wkb:Some(row.wkb),attributes:serde_json::json!({"attributes":row.attributes,"provenance":{"algorithm":r.algorithm,"parameters":r.parameters,"source_ids":row.source_ids,"overlay_ids":row.overlay_ids,"adapter":r.adapter}})});
        }
        if !mutations.is_empty(){store.apply_batch(&mutations).map_err(|e|e.to_string())?;}
        store.checkpoint().map_err(|e|e.to_string())?;drop(store);
        fs::File::open(stage.join("workspace.gpkg")).and_then(|f|f.sync_all()).map_err(|_|"SYNC_FAILED".to_owned())?;
        fs::File::open(&stage).and_then(|f|f.sync_all()).map_err(|_|"SYNC_FAILED".to_owned())?;
        if dst.exists(){return Err("WORKSPACE_EXISTS".into());}
        fs::rename(&stage,&dst).map_err(|_|"PUBLISH_FAILED".to_owned())?;
        fs::File::open(&root).and_then(|f|f.sync_all()).map_err(|_|"PUBLISH_OUTCOME_UNKNOWN".to_owned())?;
        Ok(())
    })();
    if result.is_err(){let _=fs::remove_dir_all(&stage);}result?;Ok(id)
}
struct State { engine:Engine, session:Session, workspace:Workspace, store:LocalStore }
impl Drop for State {fn drop(&mut self){let _=self.engine.close_session(self.session);}}
#[flutter_rust_bridge::frb(opaque)]
pub struct NativeWorkspace { state:Mutex<Option<State>> }
pub fn open_local_workspace(root_path:String,workspace_id:String)->Result<NativeWorkspace,String>{
    id(&workspace_id)?;
    let dir=root(&root_path)?.join(&workspace_id);
    if !fs::symlink_metadata(&dir).is_ok_and(|m|m.is_dir()) || !fs::symlink_metadata(dir.join("workspace.gpkg")).is_ok_and(|m|m.file_type().is_file()){return Err("WORKSPACE_NOT_FOUND".into());}
    let store=LocalStore::open(&dir.join("workspace.gpkg"),QUOTA).map_err(|e|e.to_string())?;
    let engine=Engine::default();
    let session=engine.open_session().map_err(|e|e.to_string())?;
    let workspace=engine.open_workspace(session).map_err(|e|e.to_string())?;
    Ok(NativeWorkspace{state:Mutex::new(Some(State{engine,session,workspace,store}))})
}
impl NativeWorkspace {
    fn with_state<T>(&self,f:impl FnOnce(&State)->Result<T,String>)->Result<T,String>{
        let mut state=self.state.lock().map_err(|_|"SESSION_FAILED".to_owned())?;
        let s=state.as_ref().ok_or_else(||"STALE_HANDLE".to_owned())?;
        match std::panic::catch_unwind(std::panic::AssertUnwindSafe(||f(s))){
            Ok(v)=>v,Err(_)=>{state.take();Err("SESSION_FAILED".into())}
        }
    }
    pub fn close(&self)->Result<(),String>{
        self.state.lock().map_err(|_|"SESSION_FAILED".to_owned())?.take();Ok(())
    }
    pub fn apply_local(&self,operation_id:String,feature_id:String,expected_revision:u64,wkb:Option<Vec<u8>>,attributes_json:String)->Result<u64,String>{
        if attributes_json.len()>256*1024 || wkb.as_ref().is_some_and(|b|b.len()>4*1024*1024){return Err("RESOURCE_LIMIT".into());}
        let m=Mutation{operation_id:id(&operation_id)?,feature_id:id(&feature_id)?,expected_revision,wkb,attributes:serde_json::from_str(&attributes_json).map_err(|_|"INVALID_ATTRIBUTES".to_owned())?};
        self.with_state(|s|s.store.apply(&m).map(|r|r.revision).map_err(|e|e.to_string()))
    }
    pub fn preview_edit(&self,kind:String,xy:Vec<f64>)->Result<Vec<u8>,String>{
        self.with_state(|_|ovg_mobile_engine::edit::draft(&kind,&xy).map_err(|e|e.to_string()))
    }
    pub fn commit_drawing(&self,kind:String,xy:Vec<f64>,attributes_json:String)->Result<String,String>{
        if attributes_json.len()>256*1024{return Err("RESOURCE_LIMIT".into());}
        self.with_state(|s|{
            let wkb=ovg_mobile_engine::edit::draft(&kind,&xy).map_err(|e|e.to_string())?;
            let feature_id=Uuid::new_v4();
            let e=ovg_local_storage::editing::Edit{feature_id,expected_revision:0,wkb:Some(wkb),attributes:serde_json::from_str(&attributes_json).map_err(|_|"INVALID_ATTRIBUTES".to_owned())?};
            s.store.commit_edit(Uuid::new_v4(),&[e]).map_err(|e|e.to_string())?;
            Ok(feature_id.to_string())
        })
    }
    pub fn undo_redo(&self,redo:bool)->Result<bool,String>{
        self.with_state(|s|s.store.history_step(redo).map_err(|e|e.to_string()))
    }
    pub fn snap_local(&self,x:f64,y:f64,tolerance_m:f64)->Result<String,String>{
        self.with_state(|s|s.store.snap_local(x,y,tolerance_m).map_err(|e|e.to_string()))
    }
    pub fn attribute_page(&self,after:String,limit:u32,contains:String)->Result<String,String>{
        self.with_state(|s|s.store.attribute_page(after.parse().map_err(|_|"INVALID_CURSOR".to_owned())?,limit,&contains).map_err(|e|e.to_string()))
    }
    pub fn search_features(&self,query:String,after:String,limit:u32)->Result<String,String>{
        self.with_state(|s|s.store.search_features(&query,after.parse().map_err(|_|"INVALID_CURSOR".to_owned())?,limit).map_err(|e|e.to_string()))
    }
    pub fn feature_json(&self,feature_id:String)->Result<String,String>{
        self.with_state(|s|s.store.feature_json(id(&feature_id)?).map_err(|e|e.to_string()))
    }
    pub fn update_attributes(&self,feature_id:String,expected_revision:String,attributes_json:String)->Result<(),String>{
        if attributes_json.len()>256*1024{return Err("RESOURCE_LIMIT".into());}
        self.with_state(|s|s.store.update_attributes(id(&feature_id)?,expected_revision.parse().map_err(|_|"INVALID_REVISION".to_owned())?,serde_json::from_str(&attributes_json).map_err(|_|"INVALID_ATTRIBUTES".to_owned())?).map_err(|e|e.to_string()))
    }
    pub fn split_feature(&self,feature_id:String,expected_revision:String,vertex_index:u32)->Result<String,String>{
        self.with_state(|s|s.store.split_feature(id(&feature_id)?,expected_revision.parse().map_err(|_|"INVALID_REVISION".to_owned())?,vertex_index as usize).map(|v|v.to_string()).map_err(|e|e.to_string()))
    }
    pub fn merge_features(&self,first_id:String,first_revision:String,second_id:String,second_revision:String)->Result<(),String>{
        self.with_state(|s|s.store.merge_features(id(&first_id)?,first_revision.parse().map_err(|_|"INVALID_REVISION".to_owned())?,id(&second_id)?,second_revision.parse().map_err(|_|"INVALID_REVISION".to_owned())?).map_err(|e|e.to_string()))
    }
    pub fn set_attribute_schema(&self,fields_json:String)->Result<(),String>{
        if fields_json.len()>256*1024{return Err("RESOURCE_LIMIT".into());}
        self.with_state(|s|{
            let fields:Vec<ovg_mobile_engine::attributes::Field>=serde_json::from_str(&fields_json).map_err(|_|"INVALID_SCHEMA".to_owned())?;
            s.store.set_attribute_schema(&fields).map_err(|e|e.to_string())
        })
    }
    pub fn list_tracks(&self)->Result<String,String>{
        self.with_state(|s|s.store.list_tracks().map_err(|e|e.to_string()))
    }
    pub fn create_track(&self,name:String)->Result<String,String>{
        self.with_state(|s|s.store.create_track(&name).map(|v|v.to_string()).map_err(|e|e.to_string()))
    }
    pub fn track_state(&self,track_id:String,action:String)->Result<(),String>{
        self.with_state(|s|s.store.track_state(id(&track_id)?,&action).map_err(|e|e.to_string()))
    }
    pub fn append_fix(&self,track_id:String,fix_json:String)->Result<u64,String>{
        if fix_json.len()>4096{return Err("RESOURCE_LIMIT".into());}
        let fix=serde_json::from_str(&fix_json).map_err(|_|"INVALID_FIX".to_owned())?;
        self.with_state(|s|s.store.append_fix(id(&track_id)?,&fix).map_err(|e|e.to_string()))
    }
    pub fn recover_tracks(&self)->Result<u32,String>{
        self.with_state(|s|s.store.recover_tracks().map(|v|v as u32).map_err(|e|e.to_string()))
    }
    pub fn track_gpx(&self,track_id:String)->Result<String,String>{
        self.with_state(|s|s.store.track_gpx(id(&track_id)?).map_err(|e|e.to_string()))
    }
    pub fn viewport_geojson(&self,west:f64,south:f64,east:f64,north:f64)->Result<String,String>{
        self.with_state(|s|s.store.viewport_geojson([west,south,east,north]).map_err(|e|e.to_string()))
    }
    pub fn import_geojson(&self,bytes:Vec<u8>)->Result<u32,String>{
        self.with_state(|s|s.store.import_geojson(&bytes).map_err(|e|e.to_string()))
    }
    pub fn import_csv(&self,bytes:Vec<u8>,x:String,y:String,epsg:u32,delimiter:u8)->Result<u32,String>{
        self.with_state(|s|s.store.import_csv(&bytes,&x,&y,epsg,delimiter).map_err(|e|e.to_string()))
    }
    pub fn export_geojson(&self)->Result<String,String>{
        self.with_state(|s|s.store.export_geojson().map_err(|e|e.to_string()))
    }
    pub fn checkpoint(&self)->Result<(),String>{
        self.with_state(|s|s.store.checkpoint().map_err(|e|e.to_string()))
    }
    pub fn start_local_processing(&self,algorithm:String,input_ids:Vec<String>,overlay_ids:Vec<String>,parameters_json:String)->Result<NativeProcessingJob,String>{
        if parameters_json.len()>65536||input_ids.len()>128||overlay_ids.len()>128{return Err("RESOURCE_LIMIT".into());}
        let algorithm=serde_json::from_value::<ovg_mobile_engine::AlgorithmId>(serde_json::Value::String(algorithm)).map_err(|_|"UNKNOWN_ALGORITHM".to_owned())?;
        let params=serde_json::from_str(&parameters_json).map_err(|_|"INVALID_PARAMETERS".to_owned())?;
        let input_ids=input_ids.iter().map(|v|id(v)).collect::<Result<Vec<_>,_>>()?;
        let overlay_ids=overlay_ids.iter().map(|v|id(v)).collect::<Result<Vec<_>,_>>()?;
        self.with_state(|s|{
            let (mut a,mut b)=s.store.processing_inputs(&input_ids,&overlay_ids).map_err(|e|e.to_string())?;
            let epsg=if algorithm==ovg_mobile_engine::AlgorithmId::Buffer{
                let projected=ovg_mobile_engine::processing::run(ovg_mobile_engine::AlgorithmId::Reproject,4326,&a,&[],&serde_json::json!({"target_srid":3857}),||false,|_|{}).map_err(|e|e.to_string())?;
                a=projected.rows.into_iter().map(|row|ovg_mobile_engine::processing::Input{id:row.source_ids[0].clone(),wkb:row.wkb,attributes:row.attributes}).collect();3857
            }else{4326};
            let handle=s.engine.start_processing(s.workspace,algorithm,epsg,std::mem::take(&mut a),std::mem::take(&mut b),params).map_err(|e|e.to_string())?;
            Ok(NativeProcessingJob{engine:s.engine.clone(),handle})
        })
    }
    pub fn start_transform(&self,source_epsg:u32,target_epsg:u32,xy:Vec<f64>)->Result<NativeJob,String>{
        self.with_state(|s|{
            let handle=s.engine.start_transform(s.workspace,source_epsg,target_epsg,xy).map_err(|e|e.to_string())?;
            Ok(NativeJob{engine:s.engine.clone(),handle})
        })
    }
}
#[flutter_rust_bridge::frb(opaque)]
pub struct NativeJob {engine:Engine,handle:Job}
impl NativeJob {
    pub fn is_finished(&self)->Result<bool,String>{self.engine.is_finished(self.handle).map_err(|e|e.to_string())}
    pub fn take_result(&self)->Result<Vec<f64>,String>{self.engine.take_result(self.handle).map_err(|e|e.to_string())}
    pub fn cancel(&self)->Result<(),String>{self.engine.cancel(self.handle).map_err(|e|e.to_string())}
    pub fn close(&self)->Result<(),String>{self.engine.dispose_job(self.handle).map_err(|e|e.to_string())}
}
impl Drop for NativeJob {fn drop(&mut self){let _=self.engine.dispose_job(self.handle);}}

#[flutter_rust_bridge::frb(opaque)]
pub struct NativeProcessingJob{engine:Engine,handle:Job}
impl NativeProcessingJob{
    pub fn is_finished(&self)->Result<bool,String>{self.engine.is_finished(self.handle).map_err(|e|e.to_string())}
    pub fn progress(&self)->Result<u32,String>{self.engine.progress(self.handle).map_err(|e|e.to_string())}
    pub fn cancel(&self)->Result<(),String>{self.engine.cancel(self.handle).map_err(|e|e.to_string())}
    pub fn take_processing_json(&self)->Result<String,String>{
        let r=self.engine.take_processing(self.handle).map_err(|e|e.to_string())?;
        serde_json::to_string(&r).map_err(|_|"SERIALIZE_RESULT".into())
    }
    pub fn close(&self)->Result<(),String>{self.engine.dispose_job(self.handle).map_err(|e|e.to_string())}
}
impl Drop for NativeProcessingJob{fn drop(&mut self){let _=self.engine.dispose_job(self.handle);}}

// Shared standalone Unified Store candidate. No automatic replacement of legacy or managed data.
use ovg_project_store::mobile as unified;
fn unified_error(e:ovg_project_store::Error)->String{format!("UNIFIED_{e:?}")}
#[flutter_rust_bridge::frb(opaque)]
pub struct NativeUnifiedProject{inner:unified::NativeProject}
pub fn list_unified_projects(root_path:String)->Result<Vec<String>,String>{unified::list_projects(Path::new(&root_path)).map_err(unified_error)}
pub fn create_unified_project(root_path:String)->Result<String,String>{unified::create_project(Path::new(&root_path)).map_err(unified_error)}
pub fn open_unified_project(root_path:String,project_id:String)->Result<NativeUnifiedProject,String>{
 Ok(NativeUnifiedProject{inner:unified::open_project(Path::new(&root_path),&project_id).map_err(unified_error)?})
}
pub fn restore_unified_project(root_path:String,exchange_root:String,name:String)->Result<String,String>{
 unified::restore_project(Path::new(&root_path),Path::new(&exchange_root),&name).map_err(unified_error)
}
impl NativeUnifiedProject{
 pub fn close(&self)->Result<(),String>{self.inner.close().map_err(unified_error)}
 pub fn catalog(&self)->Result<String,String>{self.inner.catalog().map_err(unified_error)}
 pub fn add_layer(&self,layer_id:String,name:String,expected:String)->Result<String,String>{
  self.inner.add_layer(&layer_id,&name,&expected).map_err(unified_error)
 }
 pub fn apply_batch(&self,batch_json:String)->Result<String,String>{self.inner.apply(&batch_json).map_err(unified_error)}
 pub fn draw_point(&self,layer_id:String,feature_id:String,operation_id:String,expected:String,lon:f64,lat:f64,attributes_json:String)->Result<String,String>{
  if attributes_json.len()>256*1024{return Err("RESOURCE_LIMIT".into())}
  let layer=id(&layer_id)?;let feature=id(&feature_id)?;let operation=id(&operation_id)?;
  let rev=expected.parse::<u64>().map_err(|_|"INVALID_REVISION".to_owned())?;
  let wkb=ovg_mobile_engine::edit::draft("point",&[lon,lat]).map_err(|e|e.to_string())?;
  let attributes:serde_json::Value=serde_json::from_str(&attributes_json).map_err(|_|"INVALID_ATTRIBUTES".to_owned())?;
  let b=serde_json::json!({"operation":operation,"expected_project":rev,"edits":[{"layer":layer,"feature":feature,"expected":0,"image":{"wkb":wkb,"attributes":attributes}}],"links":[]});
  self.inner.apply(&b.to_string()).map_err(unified_error)
 }
 pub fn compensate(&self,operation_id:String,expected:String)->Result<String,String>{self.inner.compensate(&operation_id,&expected).map_err(unified_error)}
 pub fn query(&self,layer_id:String,west:f64,south:f64,east:f64,north:f64,after:String,limit:u32,expected:String)->Result<String,String>{
  self.inner.query(&layer_id,[west,south,east,north],&after,limit,&expected).map_err(unified_error)
 }
 pub fn integrity(&self)->Result<bool,String>{self.inner.integrity().map_err(unified_error)}
 pub fn backup(&self,exchange_root:String,expected:String)->Result<String,String>{
  self.inner.backup(Path::new(&exchange_root),&expected).map_err(unified_error)
 }
}
