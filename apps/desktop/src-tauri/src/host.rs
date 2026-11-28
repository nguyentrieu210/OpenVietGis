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

#[derive(Clone,Serialize,serde::Deserialize)]
pub struct LayerInfo {pub id:Uuid,pub name:String,pub source_epsg:Option<i32>,pub source_table:Option<String>}
#[derive(Clone, Serialize)]
pub struct Project { pub id: Uuid }
#[derive(Serialize)]
pub struct Opened { pub id: Uuid, pub handle: Uuid }
#[derive(Serialize)]
#[serde(tag="status", rename_all="snake_case")]
pub enum Poll { Pending, Complete { xy: Vec<f64> } }
struct Open { engine: Workspace, _store: LocalStore, project: Uuid, layer:Option<Uuid>, generation: u64, cache: ovg_mobile_engine::viewport::Cache, raster: Option<(Uuid,ovg_local_storage::mbtiles::Archive,tempfile::NamedTempFile)>, assets: HashMap<Uuid,Asset>, catalog:Option<(Uuid,crate::gpkg_import::Catalog)>, direct:Option<(Uuid,crate::gpkg_import::Direct)> }
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
enum Asset { Cad(crate::resource_files::Cad), Las(crate::resource_files::Las), Compressed(crate::compressed_points::Cloud), Glb(crate::resource_files::Glb) }
#[derive(Serialize)]
pub struct AssetInfo {pub id:Uuid,pub kind:String,pub count:u64,pub epsg:Option<u32>}
#[derive(Serialize)]
pub struct Raster { pub id:Uuid, pub attribution:String }
impl Host {
    fn read<T>(&self,handle:Uuid,f:impl FnOnce(&Open)->Result<T>)->Result<T>{
        let s=self.lock()?; f(s.open.get(&handle).ok_or("STALE_HANDLE")?)
    }
    fn mutate<T>(&self,handle:Uuid,f:impl FnOnce(&LocalStore)->Result<T>)->Result<T>{
        let mut s=self.lock()?;let w=s.open.get_mut(&handle).ok_or("STALE_HANDLE")?;
        let next=w.generation.checked_add(1).ok_or("RESOURCE_LIMIT")?;
        let out=f(&w._store)?;w.generation=next;w.cache.invalidate_layer(&handle.to_string(),&handle.to_string());Ok(out)
    }
    // Session-scoped, explicitly selected raster archive. No persistent path grants.
    pub fn open_raster(&self,handle:Uuid,path:&Path)->Result<Raster>{
        regular(path)?;
        let staged=crate::gpkg_import::sqlite_snapshot(path,512*1024*1024)?;
        let archive=ovg_local_storage::mbtiles::Archive::open(staged.path()).map_err(store_err)?;
        if archive.format!="png"{return Err("UNSUPPORTED_FORMAT")}
        let id=Uuid::new_v4();let attribution=archive.attribution.clone();
        let mut s=self.lock()?;let w=s.open.get_mut(&handle).ok_or("STALE_HANDLE")?;
        w.raster=Some((id,archive,staged));Ok(Raster{id,attribution})
    }
    pub fn raster_tile(&self,handle:Uuid,id:Uuid,z:u8,x:u32,y:u32)->Result<Vec<u8>>{
        if z>22||x>=(1u32<<z)||y>=(1u32<<z){return Err("INVALID_TILE")}
        self.read(handle,|w|{
            let (_,a,_)=w.raster.as_ref().filter(|(key,_,_)|*key==id).ok_or("STALE_HANDLE")?;
            let b=a.tile(z,x,y).map_err(store_err)?.ok_or("TILE_NOT_FOUND")?;
            if b.len()<33||b.len()>4*1024*1024||b[..8]!=[137,80,78,71,13,10,26,10]||&b[12..16]!=b"IHDR"{return Err("INVALID_TILE")}
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
    #[cfg(test)]
    pub fn vector_tile(&self,handle:Uuid,id:Uuid,revision:u64,z:u8,x:u32,y:u32)->Result<Vec<u8>>{
        self.vector_tile_cancel(handle,id,revision,z,x,y,&std::sync::atomic::AtomicBool::new(false))
    }
    pub fn vector_tile_cancel(&self,handle:Uuid,id:Uuid,revision:u64,z:u8,x:u32,y:u32,cancel:&std::sync::atomic::AtomicBool)->Result<Vec<u8>>{
        use std::sync::atomic::Ordering;
        if cancel.load(Ordering::Acquire){return Err("CANCELLED")}
        if id!=handle{return Err("STALE_HANDLE")}
        let mut s=self.lock()?;let w=s.open.get_mut(&handle).ok_or("STALE_HANDLE")?;
        if revision!=w.generation{return Err("STALE_REVISION")}
        let key=ovg_mobile_engine::viewport::Key{workspace:handle.to_string(),layer:id.to_string(),revision,style:0,zoom:z,x,y};
        if cancel.load(Ordering::Acquire){return Err("CANCELLED")}
        if let Some(b)=w.cache.get(&key){return Ok(b.to_vec())}
        let data=crate::mvt::tile(&w._store,z,x,y,cancel)?;
        if cancel.load(Ordering::Acquire){return Err("CANCELLED")}
        let epoch=w.cache.next_epoch().map_err(err)?;
        w.cache.insert(epoch,key,Arc::from(data.clone())).map_err(err)?;
        Ok(data)
    }
    pub fn tilejson(&self,handle:Uuid,id:Uuid,revision:u64)->Result<Vec<u8>>{
        if id!=handle{return Err("STALE_HANDLE")}
        self.read(handle,|w|{
            if revision!=w.generation{return Err("STALE_REVISION")}
            serde_json::to_vec(&json!({"tilejson":"3.0.0","name":"local","minzoom":0,"maxzoom":22,
              "tiles":[format!("ovg://v1/workspaces/{handle}/layers/{id}/revisions/{revision}/tiles/{{z}}/{{x}}/{{y}}.mvt")],
              "vector_layers":[{"id":"local","fields":{"stable_id":"String"},"minzoom":0,"maxzoom":22}]})).map_err(|_|"INVALID_LOCAL_DATA")
        })
    }
    pub fn direct_open(&self,handle:Uuid,path:&Path)->Result<Value>{
        let d=crate::gpkg_import::Direct::open(path)?;
        let mut s=self.lock()?;let w=s.open.get_mut(&handle).ok_or("STALE_HANDLE")?;
        let id=Uuid::new_v4();let v=json!({"id":id,"layers":d.layers});w.direct=Some((id,d));Ok(v)
    }
    pub fn direct_page(&self,handle:Uuid,grant:Uuid,table:&str,bbox:[f64;4],after:&str)->Result<crate::gpkg_import::DirectPage>{
        self.read(handle,|w|{
            let (_,d)=w.direct.as_ref().filter(|(id,_)|*id==grant).ok_or("STALE_HANDLE")?;
            d.page(table,bbox,after)
        })
    }
    pub fn direct_close(&self,handle:Uuid,grant:Uuid)->Result<()>{
        let mut s=self.lock()?;let w=s.open.get_mut(&handle).ok_or("STALE_HANDLE")?;
        if w.direct.as_ref().map(|(id,_)|*id)!=Some(grant){return Err("STALE_HANDLE")}
        w.direct=None;Ok(())
    }
    pub fn integrity_report(&self,handle:Uuid)->Result<Value>{
        self.read(handle,|w|{
            let tmp=tempfile::tempdir().map_err(|_|"STORAGE_UNAVAILABLE")?;
            let path=tmp.path().join("check.gpkg");
            w._store.export_snapshot(&path).map_err(store_err)?;
            let db=rusqlite::Connection::open_with_flags(&path,rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY).map_err(|_|"INVALID_STORAGE")?;
            let quick:String=db.pragma_query_value(None,"quick_check",|r|r.get(0)).map_err(|_|"INVALID_STORAGE")?;
            let fk:i64=db.query_row("SELECT count(*) FROM pragma_foreign_key_check",[],|r|r.get(0)).map_err(|_|"INVALID_STORAGE")?;
            let missing:i64=db.query_row("SELECT count(*) FROM features f LEFT JOIN rtree_features_geom i ON i.id=f.fid WHERE i.id IS NULL",[],|r|r.get(0)).map_err(|_|"INVALID_STORAGE")?;
            let orphan:i64=db.query_row("SELECT count(*) FROM rtree_features_geom i LEFT JOIN features f ON f.fid=i.id WHERE f.fid IS NULL",[],|r|r.get(0)).map_err(|_|"INVALID_STORAGE")?;
            let features:i64=db.query_row("SELECT count(*) FROM features",[],|r|r.get(0)).map_err(|_|"INVALID_STORAGE")?;
            let pages:i64=db.pragma_query_value(None,"page_count",|r|r.get(0)).map_err(|_|"INVALID_STORAGE")?;
            let free:i64=db.pragma_query_value(None,"freelist_count",|r|r.get(0)).map_err(|_|"INVALID_STORAGE")?;
            Ok(json!({"ok":quick=="ok"&&fk==0&&missing==0&&orphan==0,"quick_check":quick,"foreign_keys":fk,"missing_index":missing,"orphan_index":orphan,"features":features,"pages":pages,"free_pages":free}))
        })
    }
    pub fn inspect_gpkg(&self,handle:Uuid,path:&Path)->Result<Value>{
        let catalog=crate::gpkg_import::Catalog::open(path)?;
        let mut s=self.lock()?;let w=s.open.get_mut(&handle).ok_or("STALE_HANDLE")?;
        let id=Uuid::new_v4();let value=json!({"id":id,"layers":catalog.layers});
        w.catalog=Some((id,catalog));Ok(value)
    }
    pub fn import_layer(&self,handle:Uuid,grant:Uuid,table:&str)->Result<u32>{
        let mut s=self.lock()?;let w=s.open.get_mut(&handle).ok_or("STALE_HANDLE")?;
        let (_,c)=w.catalog.as_ref().filter(|(id,_)|*id==grant).ok_or("STALE_HANDLE")?;
        let next=w.generation.checked_add(1).ok_or("RESOURCE_LIMIT")?;
        let n=c.import(&w._store,table)?;
        w.generation=next;w.cache.invalidate_layer(&handle.to_string(),&handle.to_string());Ok(n)
    }
    fn layers_dir(&self,project:Uuid)->PathBuf{self.root.join(project.to_string()).join("layers")}
    fn layer_path(&self,project:Uuid,layer:Uuid)->PathBuf{self.layers_dir(project).join(layer.to_string()).join("workspace.gpkg")}
    fn layer_info(&self,project:Uuid,layer:Uuid)->Result<LayerInfo>{
        real_dir(&self.layers_dir(project))?;
        let dir=self.layers_dir(project).join(layer.to_string());real_dir(&dir)?;
        regular(&dir.join("layer.json"))?;regular(&dir.join("workspace.gpkg"))?;
        let mut data=Vec::new();fs::File::open(dir.join("layer.json")).map_err(|_|"STORAGE_UNAVAILABLE")?.take(4097).read_to_end(&mut data).map_err(|_|"INVALID_LAYER")?;
        if data.len()>4096{return Err("INVALID_LAYER")}
        let info:LayerInfo=serde_json::from_slice(&data).map_err(|_|"INVALID_LAYER")?;
        if info.id!=layer||info.name.is_empty()||info.name.len()>256{return Err("INVALID_LAYER")}
        Ok(info)
    }
    fn list_layers_for(&self,project:Uuid)->Result<Vec<LayerInfo>>{
            let dir=self.layers_dir(project);match fs::symlink_metadata(&dir){
              Err(e) if e.kind()==std::io::ErrorKind::NotFound=>return Ok(Vec::new()),
              Err(_)=>return Err("STORAGE_UNAVAILABLE"),Ok(_)=>real_dir(&dir)?
            }
            let mut out=Vec::new();
            for entry in fs::read_dir(&dir).map_err(|_|"STORAGE_UNAVAILABLE")?{
                let entry=entry.map_err(|_|"STORAGE_UNAVAILABLE")?;
                let Some(name)=entry.file_name().to_str().map(str::to_owned)else{continue};
                let Ok(id)=Uuid::parse_str(&name)else{continue};
                if name!=id.to_string(){continue}
                if out.len()>=32{return Err("RESOURCE_LIMIT")}
                out.push(self.layer_info(project,id)?);
            }
            out.sort_by(|a,b|a.name.cmp(&b.name));Ok(out)
    }
    pub fn list_layers(&self,handle:Uuid)->Result<Vec<LayerInfo>>{self.read(handle,|w|self.list_layers_for(w.project))}
    pub fn create_layer(&self,handle:Uuid,name:String,catalog:Option<(Uuid,String)>)->Result<LayerInfo>{
        if name.trim().is_empty()||name.len()>256{return Err("INVALID_LAYER_NAME")}
        let mut s=self.lock()?;let w=s.open.get_mut(&handle).ok_or("STALE_HANDLE")?;
        if w.layer.is_some(){return Err("ROOT_WORKSPACE_REQUIRED")}
        let project=w.project;
        let dir=self.layers_dir(project);fs::create_dir_all(&dir).map_err(|_|"STORAGE_UNAVAILABLE")?;real_dir(&dir)?;
        let n=fs::read_dir(&dir).map_err(|_|"STORAGE_UNAVAILABLE")?.take(33).count();
        if n>=32{return Err("RESOURCE_LIMIT")}
        let (source_epsg,source_table)=if let Some((grant,table))=&catalog{
            let (_,c)=w.catalog.as_ref().filter(|(id,_)|id==grant).ok_or("STALE_HANDLE")?;
            let l=c.layers.iter().find(|l|&l.table==table).ok_or("LAYER_NOT_FOUND")?;
            (Some(l.epsg),Some(l.table.clone()))
        }else{(None,None)};
        let info=LayerInfo{id:Uuid::new_v4(),name:name.trim().to_owned(),source_epsg,source_table};
        let stage=tempfile::tempdir_in(&dir).map_err(|_|"STORAGE_UNAVAILABLE")?;
        let p=stage.path().join("workspace.gpkg");
        {
            let store=LocalStore::open(&p,QUOTA).map_err(store_err)?;
            if let Some((grant,table))=&catalog{
                let (_,c)=w.catalog.as_ref().filter(|(id,_)|id==grant).ok_or("STALE_HANDLE")?;
                c.import(&store,table)?;
            }
            store.checkpoint().map_err(store_err)?;
        }
        let mut meta=fs::OpenOptions::new().write(true).create_new(true).open(stage.path().join("layer.json")).map_err(|_|"STORAGE_UNAVAILABLE")?;
        serde_json::to_writer(&mut meta,&info).map_err(|_|"INVALID_LAYER")?;
        meta.sync_all().map_err(|_|"STORAGE_UNAVAILABLE")?;drop(meta);
        fs::rename(stage.path(),dir.join(info.id.to_string())).map_err(|_|"STORAGE_UNAVAILABLE")?;
        Ok(info)
    }
    pub fn open_layer(&self,root:Uuid,layer:Uuid)->Result<Opened>{
        let mut s=self.lock()?;
        let base=s.open.get(&root).ok_or("STALE_HANDLE")?;
        if base.layer.is_some(){return Err("ROOT_WORKSPACE_REQUIRED")}
        let project=base.project;
        if s.open.len()>=MAX_OPEN{return Err("RESOURCE_LIMIT")}
        if s.open.values().any(|w|w.project==project&&w.layer==Some(layer)){return Err("LAYER_ALREADY_OPEN")}
        self.layer_info(project,layer)?;
        let path=self.layer_path(project,layer);
        for suffix in ["-wal","-shm","-journal"]{
            let mut side=path.as_os_str().to_os_string();side.push(suffix);
            match fs::symlink_metadata(side){
                Ok(m) if !m.file_type().is_file()=>return Err("INVALID_STORAGE"),
                Err(e) if e.kind()!=std::io::ErrorKind::NotFound=>return Err("INVALID_STORAGE"),
                _=>{}
            }
        }
        let store=LocalStore::open(&path,QUOTA).map_err(store_err)?;
        let cache=ovg_mobile_engine::viewport::Cache::new(16*1024*1024).map_err(err)?;
        let workspace=self.engine.open_workspace(s.session).map_err(err)?;
        let handle=Uuid::new_v4();
        s.open.insert(handle,Open{engine:workspace,_store:store,project,layer:Some(layer),generation:0,cache,raster:None,assets:HashMap::new(),catalog:None,direct:None});
        Ok(Opened{id:project,handle})
    }
    pub fn import_gpkg(&self,handle:Uuid,path:&Path)->Result<u32>{
        self.mutate(handle,|s|crate::gpkg_import::import(s,path))
    }
    pub fn open_asset(&self,handle:Uuid,path:&Path,kind:&str,epsg:Option<u32>)->Result<AssetInfo>{
        // Explicit source CRS. CAD adapter only accepts WGS84 XY; LAS emits
        // native XYZ and must not be displayed as degrees without reprojection.
        let asset=match kind{
            "cad" if epsg==Some(4326)=>Asset::Cad(crate::resource_files::Cad::open(path)?),
            "las" if epsg.is_some_and(|n|n>0)=>Asset::Las(crate::resource_files::Las::open(path)?),
            "laz"|"copc" if epsg.is_some_and(|n|n>0)=>Asset::Compressed(crate::compressed_points::Cloud::open(path,kind=="copc")?),
            "glb" if epsg.is_none()=>Asset::Glb(crate::resource_files::Glb::open(path)?),
            _=>return Err("EXPLICIT_CRS_OR_FORMAT_REQUIRED")
        };
        let count=match &asset{Asset::Cad(a)=>a.count(),Asset::Las(a)=>a.count(),Asset::Compressed(a)=>a.count(),Asset::Glb(a)=>a.len()};
        let mut s=self.lock()?;let w=s.open.get_mut(&handle).ok_or("STALE_HANDLE")?;
        if w.assets.len()>=3{return Err("RESOURCE_LIMIT")}
        let id=Uuid::new_v4();w.assets.insert(id,asset);
        Ok(AssetInfo{id,kind:kind.into(),count,epsg})
    }
    pub fn copc_query(&self,handle:Uuid,id:Uuid,bounds:[f64;6],level:i32)->Result<Vec<u8>>{
        self.read(handle,|w|match w.assets.get(&id){
            Some(Asset::Compressed(a))=>a.query(bounds,level),_=>Err("STALE_HANDLE")
        })
    }
    pub fn cad_viewport(&self,handle:Uuid,id:Uuid,bbox:[f64;4])->Result<Vec<u8>>{
        self.read(handle,|w|match w.assets.get(&id){
            Some(Asset::Cad(a))=>a.viewport(bbox),_=>Err("STALE_HANDLE")
        })
    }
    pub fn close_asset(&self,handle:Uuid,id:Uuid)->Result<()>{
        let mut s=self.lock()?;let w=s.open.get_mut(&handle).ok_or("STALE_HANDLE")?;
        w.assets.remove(&id).ok_or("STALE_HANDLE")?;Ok(())
    }
    pub fn asset_resource(&self,handle:Uuid,id:Uuid,resource:crate::protocol::Resource)->Result<Vec<u8>>{
        let mut s=self.lock()?;let w=s.open.get_mut(&handle).ok_or("STALE_HANDLE")?;
        let a=w.assets.get_mut(&id).ok_or("STALE_HANDLE")?;
        let b=match(a,resource){
            (Asset::Cad(a),crate::protocol::Resource::CadChunk(n))=>a.chunk(n)?,
            (Asset::Las(a),crate::protocol::Resource::PointNode(n))=>a.chunk(n)?,
            (Asset::Compressed(a),crate::protocol::Resource::PointNode(n))=>a.chunk(n)?,
            (Asset::Glb(a),crate::protocol::Resource::Asset)=>a.bytes(),
            _=>return Err("UNSUPPORTED_RESOURCE")
        };
        if b.len()>4*1024*1024{return Err("RESOURCE_LIMIT")}Ok(b)
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
        self.read(handle,|w|{
            let n=if after=="0"{0}else{
                let (generation,fid)=after.split_once(':').ok_or("INVALID_CURSOR")?;
                if generation!=w.generation.to_string(){return Err("STALE_REVISION")}
                fid.parse::<i64>().map_err(|_|"INVALID_CURSOR")?
            };
            let raw=w._store.attribute_page(n,limit,&contains).map_err(store_err)?;
            if raw.len()>MAX_READ{return Err("RESOURCE_LIMIT")}
            let mut v:Value=serde_json::from_str(&raw).map_err(|_|"INVALID_LOCAL_DATA")?;
            for row in v.as_array_mut().ok_or("INVALID_LOCAL_DATA")?{
                for field in ["fid","revision"]{let val=row[field].as_u64().ok_or("INVALID_LOCAL_DATA")?;row[field]=json!(val.to_string());}
            }
            let next=v.as_array().and_then(|r|r.last()).and_then(|r|r["fid"].as_str()).map(|fid|format!("{}:{fid}",w.generation));
            Ok(json!({"generation":w.generation.to_string(),"rows":v,"next":next}))
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
    pub fn reshape(&self,handle:Uuid,id:Uuid,revision:String,geometry:Value)->Result<()>{
        let rev=revision.parse::<u64>().map_err(|_|"INVALID_REVISION")?;
        if geometry.to_string().len()>1024*1024{return Err("RESOURCE_LIMIT")}
        let kind=geometry["type"].as_str().ok_or("INVALID_GEOMETRY")?;
        if !matches!(kind,"Point"|"MultiPoint"|"LineString"|"MultiLineString"|"Polygon"|"MultiPolygon"){return Err("UNSUPPORTED_GEOMETRY")}
        fn xy(v:&Value,depth:usize)->bool{
            if depth>5{return false}
            let Some(a)=v.as_array()else{return false};
            if a.is_empty(){return false}
            if a[0].is_number(){return a.len()==2&&a.iter().all(|n|n.as_f64().is_some_and(f64::is_finite))}
            a.iter().all(|n|xy(n,depth+1))
        }
        if !xy(&geometry["coordinates"],0){return Err("INVALID_GEOMETRY")}
        let g=ovg_spatial::parse_geojson(&geometry,&ovg_spatial::ValidationLimits::default()).map_err(|_|"INVALID_GEOMETRY")?;
        let bytes=ovg_spatial::encode_wkb(&g,None).map_err(|_|"INVALID_GEOMETRY")?;
        ovg_mobile_engine::edit::validate(&bytes).map_err(err)?;
        self.mutate(handle,|s|{
            let raw=s.feature_json(id).map_err(store_err)?;
            let feature:Value=serde_json::from_str(&raw).map_err(|_|"INVALID_LOCAL_DATA")?;
            if feature["revision"].as_u64()!=Some(rev){return Err("REVISION_CONFLICT")}
            s.commit_edit(Uuid::new_v4(),&[Edit{feature_id:id,expected_revision:rev,wkb:Some(bytes),attributes:feature["attributes"].clone()}]).map_err(store_err)
        })
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
    pub fn project_state(&self,handle:Uuid)->Result<crate::project_state::ProjectState>{
        self.read(handle,|w|{
            let ids:Vec<_>=self.list_layers_for(w.project)?.iter().map(|l|l.id).collect();
            crate::project_state::read(&self.root.join(w.project.to_string()),&ids)
        })
    }
    pub fn save_project_state(&self,handle:Uuid,state:crate::project_state::ProjectState,expected:u64)->Result<crate::project_state::ProjectState>{
        self.read(handle,|w|{
            if w.layer.is_some(){return Err("ROOT_WORKSPACE_REQUIRED")}
            let ids:Vec<_>=self.list_layers_for(w.project)?.iter().map(|l|l.id).collect();
            crate::project_state::save(&self.root.join(w.project.to_string()),&ids,state,expected)
        })
    }
    pub fn set_schema(&self,handle:Uuid,fields:Vec<ovg_mobile_engine::attributes::Field>)->Result<()>{
        self.mutate(handle,|s|s.set_attribute_schema(&fields).map_err(store_err))
    }
    fn snapshot_project(&self,handle:Uuid,stage:&Path)->Result<(Uuid,Vec<LayerInfo>,crate::project_state::ProjectState)>{
        let s=self.lock()?;let w=s.open.get(&handle).ok_or("STALE_HANDLE")?;
        if w.layer.is_some(){return Err("ROOT_WORKSPACE_REQUIRED")}
        let project=w.project;let layers=self.list_layers_for(project)?;
        w._store.export_snapshot(&stage.join("workspace.gpkg")).map_err(store_err)?;
        let mut total=fs::metadata(stage.join("workspace.gpkg")).map_err(|_|"STORAGE_UNAVAILABLE")?.len();
        for layer in &layers{
            let dir=stage.join("layers").join(layer.id.to_string());
            fs::create_dir_all(&dir).map_err(|_|"STORAGE_UNAVAILABLE")?;
            let snapshot=dir.join("workspace.gpkg");
            if let Some(open)=s.open.values().find(|o|o.project==project&&o.layer==Some(layer.id)){
                open._store.export_snapshot(&snapshot).map_err(store_err)?;
            }else{
                let p=self.layer_path(project,layer.id);
                for suffix in ["-wal","-shm","-journal"]{
                    let mut side=p.as_os_str().to_os_string();side.push(suffix);
                    match fs::symlink_metadata(side){Ok(m) if !m.file_type().is_file()=>return Err("INVALID_STORAGE"),Err(e) if e.kind()!=std::io::ErrorKind::NotFound=>return Err("INVALID_STORAGE"),_=>{}}
                }
                let store=LocalStore::open(&p,QUOTA).map_err(store_err)?;
                store.export_snapshot(&snapshot).map_err(store_err)?;
            }
            total=total.checked_add(fs::metadata(&snapshot).map_err(|_|"STORAGE_UNAVAILABLE")?.len()).ok_or("RESOURCE_LIMIT")?;
            if total>2*1024*1024*1024{return Err("RESOURCE_LIMIT")}
        }
        let ids:Vec<_>=layers.iter().map(|l|l.id).collect();
        let state=crate::project_state::read(&self.root.join(project.to_string()),&ids)?;
        Ok((project,layers,state))
    }
    pub fn backup_project(&self,handle:Uuid,dest:&Path)->Result<()>{
        let stage=tempfile::tempdir().map_err(|_|"STORAGE_UNAVAILABLE")?;
        let (_,layers,state)=self.snapshot_project(handle,stage.path())?;
        crate::project_backup::write_bundle(stage.path(),&layers,&state,dest)
    }
    // Export only. Existing project is still the authoritative editable store.
    pub fn export_unified_candidate(&self,handle:Uuid,dest:&Path)->Result<Vec<ovg_project_store::migration::Migrated>>{
        if dest.exists(){return Err("DESTINATION_EXISTS")}
        let stage=tempfile::tempdir().map_err(|_|"STORAGE_UNAVAILABLE")?;
        let (project,layers,_state)=self.snapshot_project(handle,stage.path())?;
        let mut sources=vec![ovg_project_store::migration::Source{layer:project,name:"Default".into(),snapshot:stage.path().join("workspace.gpkg")}];
        for layer in layers{
            sources.push(ovg_project_store::migration::Source{layer:layer.id,name:layer.name,snapshot:stage.path().join("layers").join(layer.id.to_string()).join("workspace.gpkg")});
        }
        ovg_project_store::migration::migrate(&sources,dest,&std::sync::atomic::AtomicBool::new(false))
            .map_err(|e|match e{ovg_project_store::Error::Unsupported=>"UNSUPPORTED_MIGRATION_PROFILE",ovg_project_store::Error::Limit=>"RESOURCE_LIMIT",ovg_project_store::Error::Conflict=>"MIGRATION_CONFLICT",_=>"MIGRATION_FAILED"})
    }
    pub fn restore_project(&self,path:&Path)->Result<Project>{
        let _guard=self.lock()?;
        real_dir(&self.root)?;
        if self.list()?.len()>=MAX_PROJECTS{return Err("RESOURCE_LIMIT")}
        let id=Uuid::new_v4();let stage=tempfile::tempdir_in(&self.root).map_err(|_|"STORAGE_UNAVAILABLE")?;
        crate::project_backup::restore_bundle(path,stage.path())?;
        fs::rename(stage.path(),self.root.join(id.to_string())).map_err(|_|"STORAGE_UNAVAILABLE")?;
        Ok(Project{id})
    }
    pub fn restore_snapshot(&self,path:&Path)->Result<Project>{
        let _guard=self.lock()?;
        real_dir(&self.root)?;
        if self.list()?.len()>=MAX_PROJECTS{return Err("RESOURCE_LIMIT")}
        let id=Uuid::new_v4();
        let stage=tempfile::tempdir_in(&self.root).map_err(|_|"STORAGE_UNAVAILABLE")?;
        crate::snapshot_restore::restore(path,stage.path())?;
        fs::rename(stage.path(),self.root.join(id.to_string())).map_err(|_|"STORAGE_UNAVAILABLE")?;
        // TempDir destructor sees the old path, not the published workspace.
        Ok(Project{id})
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
        s.open.insert(handle,Open{engine:workspace,_store:store,project:id,layer:None,generation:0,cache:ovg_mobile_engine::viewport::Cache::new(16*1024*1024).map_err(err)?,raster:None,assets:HashMap::new(),catalog:None,direct:None});
        Ok(Opened{id,handle})
    }
    pub fn close(&self,handle:Uuid)->Result<()> {
        let mut s=self.lock()?;
        let open=s.open.get(&handle).ok_or("STALE_HANDLE")?;
        let project=open.project;
        let handles:Vec<_>=if open.layer.is_none(){s.open.iter().filter(|(_,w)|w.project==project).map(|(id,_)|*id).collect()}else{vec![handle]};
        // Checkpoint every layer before changing lifecycle state.
        for id in &handles{s.open.get(id).ok_or("STALE_HANDLE")?._store.checkpoint().map_err(store_err)?;}
        for id in &handles{
            let w=s.open.get(id).ok_or("STALE_HANDLE")?.engine;
            self.engine.close_workspace(w).map_err(err)?;
            s.open.remove(id);
        }
        s.jobs.retain(|_,j|!handles.contains(&j.handle));
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
