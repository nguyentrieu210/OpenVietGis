//! Independent local GeoPackage only. Managed data must never enter this plaintext adapter.
use ovg_spatial::{encode_wkb, parse_xy_wkb, validate_geometry, BBox, ValidationLimits};
use rusqlite::{functions::FunctionFlags, params, Connection, OptionalExtension, TransactionBehavior};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{fs, path::Path, sync::Mutex, time::Duration};
use uuid::Uuid;

const APP_ID: i32 = 0x47504b47;
const VERSION: i32 = 10300;
const MAX_GEOMETRY: usize = 4 * 1024 * 1024;
const MAX_ATTRIBUTES: usize = 256 * 1024;
const MIN_QUOTA: u64 = 4 * 1024 * 1024;
const MAX_QUOTA: u64 = 64 * 1024 * 1024 * 1024;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("INVALID_LOCAL_DATA")] Invalid,
    #[error("REVISION_CONFLICT")] Conflict,
    #[error("OPERATION_ID_REUSED")] Reused,
    #[error("UNSUPPORTED_DATABASE")] Unsupported,
    #[error("STORAGE_LIMIT")] Limit,
    #[error("STORAGE_UNAVAILABLE")] Unavailable,
    #[error("DATABASE_ERROR")] Db(#[from] rusqlite::Error),
    #[error("IO_ERROR")] Io(#[from] std::io::Error),
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Mutation {
    pub operation_id: Uuid,
    pub feature_id: Uuid,
    pub expected_revision: u64,
    pub wkb: Option<Vec<u8>>,
    pub attributes: serde_json::Value,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Receipt { pub revision: u64, pub replayed: bool }
#[derive(Debug)]
pub struct Feature { pub feature_id: Uuid, pub revision: u64, pub wkb: Vec<u8>, pub attributes: serde_json::Value }
pub struct LocalStore { db: Mutex<Connection> }

pub mod vector;
pub mod mbtiles;
pub mod editing;
pub mod survey;
pub mod media;
const SCHEMA: &str = include_str!("schema.sql");
fn parse_geometry(blob: &[u8]) -> Result<(Vec<u8>,BBox),Error> {
    if blob.len()>MAX_GEOMETRY {return Err(Error::Limit);}
    let geom=parse_xy_wkb(blob,&ValidationLimits::default()).map_err(|_|Error::Invalid)?;
    if geom.srid.is_some_and(|s|s!=4326){return Err(Error::Invalid);}
    let report=validate_geometry(&geom.geometry,&ValidationLimits::default()).map_err(|_|Error::Invalid)?;
    let b=report.bbox;
    if b.min_x < -180. || b.max_x > 180. || b.min_y < -90. || b.max_y > 90. {return Err(Error::Invalid);}
    // Re-encode as plain ISO WKB: SRID is stored in the standard GPKG header.
    let wkb=encode_wkb(&geom.geometry,None).map_err(|_|Error::Invalid)?;
    Ok((wkb,b))
}
fn pack(wkb:&[u8])->Vec<u8>{
    let mut v=Vec::with_capacity(8+wkb.len());
    v.extend(b"GP"); v.extend([0,1]); v.extend(4326_i32.to_le_bytes()); v.extend(wkb); v
}
fn unpack(blob:&[u8])->Result<&[u8],Error>{
    // This adapter writes only standard little-endian XY header without envelope.
    if blob.len()<9 || &blob[..4]!=b"GP\x00\x01" || blob[4..8]!=4326_i32.to_le_bytes(){return Err(Error::Invalid);}
    Ok(&blob[8..])
}
fn install_functions(db:&Connection)->Result<(),Error>{
    let flags=FunctionFlags::SQLITE_UTF8|FunctionFlags::SQLITE_DETERMINISTIC;
    for (name, axis) in [("ST_MinX",0),("ST_MaxX",1),("ST_MinY",2),("ST_MaxY",3)] {
        db.create_scalar_function(name,1,flags,move|ctx|{
            let bytes=ctx.get::<Vec<u8>>(0)?;
            let (_,b)=unpack(&bytes).and_then(parse_geometry).map_err(|e|rusqlite::Error::UserFunctionError(Box::new(e)))?;
            Ok(match axis {0=>b.min_x,1=>b.max_x,2=>b.min_y,_=>b.max_y})
        })?;
    }
    Ok(())
}
impl LocalStore {
    /// Creates a new app-owned workspace or opens one with this exact schema.
    /// Call only with a path resolved inside the app's private local workspace directory.
    pub fn open(path:&Path,quota:u64)->Result<Self,Error>{
        if !(MIN_QUOTA..=MAX_QUOTA).contains(&quota){return Err(Error::Limit);}
        if let Ok(meta)=fs::symlink_metadata(path){if !meta.file_type().is_file(){return Err(Error::Invalid);}}
        let mut db=Connection::open(path)?;
        db.busy_timeout(Duration::from_secs(3))?;
        db.pragma_update(None,"foreign_keys","ON")?;
        install_functions(&db)?;
        let app:i32=db.pragma_query_value(None,"application_id",|r|r.get(0))?;
        let version:i32=db.pragma_query_value(None,"user_version",|r|r.get(0))?;
        let tables:i64=db.query_row("SELECT count(*) FROM sqlite_master WHERE type='table'",[],|r|r.get(0))?;
        if app==0 && version==0 && tables==0 {
            let tx=db.transaction_with_behavior(TransactionBehavior::Immediate)?;
            tx.execute_batch(SCHEMA)?;
            tx.pragma_update(None,"application_id",APP_ID)?;
            tx.pragma_update(None,"user_version",VERSION)?;
            tx.commit()?;
        } else if app!=APP_ID || version!=VERSION || !matches!(db.query_row("SELECT schema_version FROM ovg_local_metadata WHERE singleton=1",[],|r|r.get::<_,i32>(0)).ok(),Some(1|2|3)) {
            return Err(Error::Unsupported);
        }
        let integrity:String=db.pragma_query_value(None,"quick_check",|r|r.get(0))?;
        if integrity!="ok"{return Err(Error::Invalid);}
        let tx=db.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let schema:i32=tx.query_row("SELECT schema_version FROM ovg_local_metadata WHERE singleton=1",[],|r|r.get(0))?;
        if schema==1{
            tx.execute_batch(include_str!("migration_002.sql"))?;
        }
        if schema<=2{tx.execute_batch(include_str!("migration_003.sql"))?;}
        tx.commit()?;
        db.pragma_update(None,"journal_mode","WAL")?;
        db.pragma_update(None,"synchronous","FULL")?;
        db.pragma_update(None,"wal_autocheckpoint",128)?;
        db.pragma_update(None,"journal_size_limit",2*1024*1024)?;
        let page_size:u64=db.pragma_query_value(None,"page_size",|r|r.get(0))?;
        let actual:u64=db.pragma_query_value(None,"page_count",|r|r.get(0))?;
        if actual>quota/page_size{return Err(Error::Limit);}
        db.pragma_update(None,"max_page_count",quota/page_size)?;
        Ok(Self{db:Mutex::new(db)})
    }
    fn lock(&self)->Result<std::sync::MutexGuard<'_,Connection>,Error>{self.db.lock().map_err(|_|Error::Unavailable)}
    /// Single ACID transaction for feature, spatial index, revision and local journal.
    /// Local journal never grants server permissions and is never automatically uploaded.
    pub fn apply(&self,m:&Mutation)->Result<Receipt,Error>{
        let mut db=self.lock()?;
        let tx=db.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let result=Self::apply_in(&tx,m)?;
        if !result.replayed{tx.execute("DELETE FROM ovg_edit_history",[])?;}
        tx.commit()?;
        Ok(result)
    }
    /// Bounded import transaction. No partial feature/journal mutation on failure.
    pub fn apply_batch(&self, mutations:&[Mutation])->Result<Vec<Receipt>,Error>{
        if mutations.is_empty() || mutations.len()>4096 {return Err(Error::Limit);}
        let mut bytes=0usize;
        for m in mutations {
            bytes=bytes.checked_add(m.wkb.as_ref().map_or(0,Vec::len)).and_then(|n|n.checked_add(m.attributes.to_string().len())).ok_or(Error::Limit)?;
            if bytes>16*1024*1024{return Err(Error::Limit);}
        }
        let mut db=self.lock()?;
        let tx=db.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let mut out=Vec::with_capacity(mutations.len());
        for m in mutations {out.push(Self::apply_in(&tx,m)?);}
        if out.iter().any(|r|!r.replayed){tx.execute("DELETE FROM ovg_edit_history",[])?;}
        tx.commit()?;
        Ok(out)
    }
    fn apply_in(tx:&rusqlite::Transaction<'_>,m:&Mutation)->Result<Receipt,Error>{
        if m.feature_id.is_nil()||m.operation_id.is_nil()||m.expected_revision>=i64::MAX as u64||!m.attributes.is_object(){return Err(Error::Invalid);}
        if m.wkb.is_some(){Self::validate_attributes_in(tx,&m.attributes)?;}
        let attrs=serde_json::to_string(&m.attributes).map_err(|_|Error::Invalid)?;
        if attrs.len()>MAX_ATTRIBUTES{return Err(Error::Limit);}
        let geometry=match &m.wkb{Some(w)=>Some(pack(&parse_geometry(w)?.0)),None=>None};
        let payload=serde_json::to_vec(m).map_err(|_|Error::Invalid)?;
        let digest=Sha256::digest(&payload).to_vec();
        let previous:Option<(Vec<u8>,u64)>=tx.query_row("SELECT payload_hash,revision FROM ovg_journal WHERE operation_id=?1",[m.operation_id.to_string()],|r|Ok((r.get(0)?,r.get(1)?))).optional()?;
        if let Some((old,revision))=previous{
            if old!=digest{return Err(Error::Reused);}
            return Ok(Receipt{revision,replayed:true});
        }
        let id=m.feature_id.to_string();
        let prior:Option<(u64,bool)>=tx.query_row("SELECT revision,deleted FROM ovg_feature_revision WHERE stable_id=?1",[&id],|r|Ok((r.get(0)?,r.get(1)?))).optional()?;
        let current=prior.map_or(0,|x|x.0);
        if current!=m.expected_revision || (geometry.is_none() && prior.is_none_or(|x|x.1)){return Err(Error::Conflict);}
        let rev=current+1;
        if let Some(geometry)=geometry{
            tx.execute("INSERT INTO features(stable_id,revision,geom,attributes) VALUES(?1,?2,?3,?4) ON CONFLICT(stable_id) DO UPDATE SET revision=excluded.revision,geom=excluded.geom,attributes=excluded.attributes",params![id,rev,geometry,attrs])?;
        }else{tx.execute("DELETE FROM features WHERE stable_id=?1",[&id])?;}
        tx.execute("INSERT INTO ovg_feature_revision(stable_id,revision,deleted) VALUES(?1,?2,?3) ON CONFLICT(stable_id) DO UPDATE SET revision=excluded.revision,deleted=excluded.deleted",params![id,rev,m.wkb.is_none()])?;
        tx.execute("INSERT INTO ovg_journal(operation_id,stable_id,payload_hash,revision,payload) VALUES(?1,?2,?3,?4,?5)",params![m.operation_id.to_string(),id,digest,rev,payload])?;
        tx.execute("UPDATE gpkg_contents SET last_change=strftime('%Y-%m-%dT%H:%M:%fZ','now') WHERE table_name='features'",[])?;
        Ok(Receipt{revision:rev,replayed:false})
    }
    pub fn query(&self,bbox:[f64;4],after_fid:i64,limit:u32)->Result<Vec<(i64,Feature)>,Error>{
        let db=self.lock()?;
        Self::query_in(&db,bbox,after_fid,limit)
    }
    fn query_in(db:&Connection,bbox:[f64;4],after_fid:i64,limit:u32)->Result<Vec<(i64,Feature)>,Error>{
        let [minx,miny,maxx,maxy]=bbox;
        if !bbox.iter().all(|n|n.is_finite())||minx>maxx||miny>maxy||after_fid<0||limit==0||limit>4096{return Err(Error::Invalid);}
        let mut stmt=db.prepare("SELECT f.fid,f.stable_id,f.revision,f.geom,f.attributes FROM rtree_features_geom r JOIN features f ON f.fid=r.id WHERE r.maxx>=?1 AND r.minx<=?2 AND r.maxy>=?3 AND r.miny<=?4 AND f.fid>?5 AND ST_MaxX(f.geom)>=?1 AND ST_MinX(f.geom)<=?2 AND ST_MaxY(f.geom)>=?3 AND ST_MinY(f.geom)<=?4 ORDER BY f.fid LIMIT ?6")?;
        let mut rows=stmt.query(params![minx,maxx,miny,maxy,after_fid,limit])?;
        let mut out=Vec::new();let mut bytes=0usize;
        while let Some(row)=rows.next()?{
            let blob:Vec<u8>=row.get(3)?;let attr:String=row.get(4)?;
            if bytes+blob.len()+attr.len()>4*1024*1024 {if out.is_empty(){return Err(Error::Limit);}break;}
            let wkb=unpack(&blob)?.to_vec();
            let (_,b)=parse_geometry(&wkb)?;
            // SQLite RTree outward float rounding may return false positives.
            if b.max_x<minx||b.min_x>maxx||b.max_y<miny||b.min_y>maxy{continue;}
            bytes+=blob.len()+attr.len();
            out.push((row.get(0)?,Feature{feature_id:Uuid::parse_str(&row.get::<_,String>(1)?).map_err(|_|Error::Invalid)?,revision:row.get(2)?,wkb,attributes:serde_json::from_str(&attr).map_err(|_|Error::Invalid)?}));
        }
        Ok(out)
    }
    /// Consistent revision-pinned input from the standalone workspace only.
    pub fn processing_inputs(&self,ids:&[Uuid],overlay:&[Uuid])->Result<(Vec<ovg_mobile_engine::processing::Input>,Vec<ovg_mobile_engine::processing::Input>),Error>{
        if ids.is_empty()||ids.len()>128||overlay.len()>128{return Err(Error::Limit);}
        let mut db=self.lock()?;let tx=db.transaction()?;
        let read=|ids:&[Uuid]|->Result<Vec<ovg_mobile_engine::processing::Input>,Error>{
            let mut out=Vec::new();let mut seen=std::collections::HashSet::new();let mut bytes=0;
            for id in ids{
                if id.is_nil()||!seen.insert(id){return Err(Error::Invalid);}
                let (revision,blob,attrs):(u64,Vec<u8>,String)=tx.query_row("SELECT revision,geom,attributes FROM features WHERE stable_id=?1",[id.to_string()],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?))).optional()?.ok_or(Error::Conflict)?;
                bytes+=blob.len()+attrs.len();if bytes>4*1024*1024{return Err(Error::Limit);}
                out.push(ovg_mobile_engine::processing::Input{id:format!("{id}@{revision}"),wkb:unpack(&blob)?.to_vec(),attributes:serde_json::from_str(&attrs).map_err(|_|Error::Invalid)?});
            }Ok(out)
        };
        let a=read(ids)?;let b=read(overlay)?;tx.commit()?;Ok((a,b))
    }
    pub fn search_features(&self,query:&str,after:i64,limit:u32)->Result<String,Error>{
        if query.trim().is_empty()||query.len()>256||after<0||limit==0||limit>100{return Err(Error::Invalid);}
        let db=self.lock()?;let mut stmt=db.prepare("SELECT fid,stable_id,revision,geom,attributes FROM features WHERE fid>?1 AND (instr(lower(stable_id),lower(?2))>0 OR instr(lower(attributes),lower(?2))>0) ORDER BY fid LIMIT ?3")?;
        let mut rows=stmt.query(params![after,query,limit])?;let mut out=Vec::new();let mut bytes=0usize;
        while let Some(r)=rows.next()?{
            let blob:Vec<u8>=r.get(3)?;let attributes:String=r.get(4)?;
            let (_,b)=parse_geometry(unpack(&blob)?)?;
            bytes+=attributes.len()+128;if bytes>1024*1024{if out.is_empty(){return Err(Error::Limit);}break;}
            out.push(serde_json::json!({"fid":r.get::<_,i64>(0)?,"id":r.get::<_,String>(1)?,"revision":r.get::<_,u64>(2)?,"bbox":[b.min_x,b.min_y,b.max_x,b.max_y],"attributes":serde_json::from_str::<serde_json::Value>(&attributes).map_err(|_|Error::Invalid)?}));
        }
        Ok(serde_json::Value::Array(out).to_string())
    }
    pub fn checkpoint(&self)->Result<(),Error>{
        let db=self.lock()?;
        let busy:i32=db.query_row("PRAGMA wal_checkpoint(TRUNCATE)",[],|r|r.get(0))?;
        if busy!=0{return Err(Error::Unavailable);} Ok(())
    }
    /// Durable snapshot. Never replaces an existing destination. Caller owns destination policy.
    pub fn export_snapshot(&self,destination:&Path)->Result<(),Error>{
        let parent=destination.parent().ok_or(Error::Invalid)?;
        let temp=parent.join(format!(".ovg-export-{}.tmp",Uuid::new_v4()));
        fs::OpenOptions::new().create_new(true).write(true).open(&temp)?;
        let result=(||->Result<(),Error>{
            let db=self.lock()?;
            let mut copy=Connection::open(&temp)?;
            {let backup=rusqlite::backup::Backup::new(&db,&mut copy)?;
             backup.run_to_completion(128,Duration::from_millis(5),None)?;}
            copy.pragma_update(None,"journal_mode","DELETE")?;
            drop(copy);
            fs::File::open(&temp)?.sync_all()?;
            fs::hard_link(&temp,destination)?; // same directory, atomic no-clobber publication
            fs::File::open(parent)?.sync_all()?;
            Ok(())
        })();
        let _=fs::remove_file(&temp);
        result
    }
}
