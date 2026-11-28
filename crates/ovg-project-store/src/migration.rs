//! Offline, explicit migration of checkpointed SPEC 073 v3 standalone snapshots.
//! New output only; legacy history remains archived, not executable in new undo stack.
use super::*;
use std::{fs,io::{Read,Write},path::PathBuf,sync::atomic::{AtomicBool,Ordering}};
const MAX_SOURCE:u64=512*1024*1024;
#[derive(Clone,Debug)]
pub struct Source{pub layer:Uuid,pub name:String,pub snapshot:PathBuf}
#[derive(Debug,Serialize)]
pub struct Migrated{pub layer:Uuid,pub live:u64,pub tombstones:u64,pub journal:u64,pub history:u64,pub sha256:String}
fn check_cancel(c:&AtomicBool)->Result<()>{if c.load(Ordering::Acquire){Err(Error::Conflict)}else{Ok(())}}
fn copy_source(src:&Path,dst:&Path,cancel:&AtomicBool)->Result<(u64,String)>{
 let meta=fs::symlink_metadata(src).map_err(|_|Error::Storage)?;
 if !meta.file_type().is_file()||meta.len()>MAX_SOURCE{return Err(Error::Limit)}
 for suffix in ["-wal","-shm","-journal"]{
  let mut p=src.as_os_str().to_os_string();p.push(suffix);
  match fs::symlink_metadata(p){Err(e) if e.kind()==std::io::ErrorKind::NotFound=>{},_=>return Err(Error::Unsupported)}
 }
 let mut input=fs::File::open(src).map_err(|_|Error::Storage)?;
 let mut out=fs::OpenOptions::new().create_new(true).write(true).open(dst).map_err(|_|Error::Storage)?;
 let mut hash=Sha256::new();let mut buf=vec![0u8;64*1024];let mut size=0u64;
 loop{check_cancel(cancel)?;let n=input.read(&mut buf).map_err(|_|Error::Storage)?;if n==0{break}
  size+=n as u64;if size>MAX_SOURCE{return Err(Error::Limit)}
  hash.update(&buf[..n]);out.write_all(&buf[..n]).map_err(|_|Error::Storage)?;
 }
 let end=fs::symlink_metadata(src).map_err(|_|Error::Storage)?;
 if size!=meta.len()||end.len()!=meta.len()||end.modified().ok()!=meta.modified().ok(){return Err(Error::Conflict)}
 out.sync_all().map_err(|_|Error::Storage)?;
 Ok((size,format!("{:x}",hash.finalize())))
}
fn open_read(p:&Path)->Result<Connection>{
 let db=Connection::open_with_flags(p,rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)?;
 db.pragma_update(None,"trusted_schema","OFF")?;db.pragma_update(None,"query_only","ON")?;Ok(db)
}
fn copy_layer(tx:&rusqlite::Transaction<'_>,source:&Source,legacy:&Connection,hash:&str,size:u64,cancel:&AtomicBool)->Result<Migrated>{
 let l=source.layer.to_string();
 let fields:Option<String>=legacy.query_row("SELECT fields_json FROM ovg_attribute_schema WHERE singleton=1",[],|r|r.get(0)).optional()?;
 if let Some(f)=&fields{attributes::validate_schema(&decode::<Vec<Field>>(f)?).map_err(|_|Error::Invalid)?;}
 tx.execute("INSERT INTO layer VALUES(?1,?2,?3)",params![l,source.name,fields])?;
 // Refuse survey data until its canonical representation is migrated; never silently discard it.
 let survey:i64=legacy.query_row("SELECT count(*) FROM ovg_survey_track",[],|r|r.get(0))?;
 let fixes:i64=legacy.query_row("SELECT count(*) FROM ovg_survey_fix",[],|r|r.get(0))?;
 if survey!=0||fixes!=0{return Err(Error::Unsupported)}
 let mut live=0u64;let mut tombstones=0u64;
 let mut q=legacy.prepare("SELECT r.stable_id,r.revision,r.deleted,f.revision,f.geom,f.attributes FROM ovg_feature_revision r LEFT JOIN features f ON f.stable_id=r.stable_id ORDER BY r.stable_id")?;
 let mut rows=q.query([])?;
 while let Some(row)=rows.next()?{
  check_cancel(cancel)?;
  let id:String=row.get(0)?;let rev:u64=row.get(1)?;let deleted:bool=row.get(2)?;
  let parsed=Uuid::parse_str(&id).map_err(|_|Error::Invalid)?;
  if parsed.is_nil()||parsed.to_string()!=id||rev==0{return Err(Error::Invalid)}
  let actual:Option<u64>=row.get(3)?;
  tx.execute("INSERT INTO feature_revision VALUES(?1,?2,?3)",params![l,id,rev])?;
  if deleted{if actual.is_some(){return Err(Error::Invalid)}tombstones+=1;continue}
  if actual!=Some(rev){return Err(Error::Invalid)}
  let raw:Vec<u8>=row.get(4)?;let attrs:String=row.get(5)?;
  if raw.len()<9||&raw[..4]!=b"GP\x00\x01"||raw[4..8]!=4326_i32.to_le_bytes(){return Err(Error::Invalid)}
  let img=Image{wkb:raw[8..].to_vec(),attributes:decode(&attrs)?};
  let (bb,a)=validate_image(tx,source.layer,&img)?;
  tx.execute("INSERT INTO feature(layer,id,wkb,attributes,minx,maxx,miny,maxy) VALUES(?1,?2,?3,?4,?5,?6,?7,?8)",params![l,id,img.wkb,a,bb[0],bb[1],bb[2],bb[3]])?;
  live+=1;
 }
 let total:u64=legacy.query_row("SELECT count(*) FROM features",[],|r|r.get(0))?;
 if total!=live{return Err(Error::Invalid)}
 let mut journal=0u64;
 let mut q=legacy.prepare("SELECT operation_id,stable_id,payload_hash,revision,payload FROM ovg_journal ORDER BY operation_id")?;
 let mut rows=q.query([])?;
 while let Some(row)=rows.next()?{
  check_cancel(cancel)?;
  let op:String=row.get(0)?;let id:String=row.get(1)?;let hash:Vec<u8>=row.get(2)?;let revision:u64=row.get(3)?;let payload:Vec<u8>=row.get(4)?;
  if payload.len()>16*1024*1024||hash.as_slice()!=&Sha256::digest(&payload)[..]{return Err(Error::Invalid)}
  let m:ovg_local_storage::Mutation=serde_json::from_slice(&payload).map_err(|_|Error::Invalid)?;
  if m.operation_id.to_string()!=op||m.feature_id.to_string()!=id||m.expected_revision.checked_add(1)!=Some(revision){return Err(Error::Invalid)}
  let current:Option<u64>=tx.query_row("SELECT revision FROM feature_revision WHERE layer=?1 AND id=?2",params![l,id],|r|r.get(0)).optional()?;
  if current.is_none_or(|r|r<revision){return Err(Error::Invalid)}
  tx.execute("INSERT INTO legacy_receipt VALUES(?1,?2,?3,?4,?5,?6)",params![l,op,id,hash,revision,payload])?;journal+=1;
 }
 let mut history=0u64;
 let mut q=legacy.prepare("SELECT seq,command_id,before_json,after_json,applied FROM ovg_edit_history ORDER BY seq")?;
 let mut rows=q.query([])?;
 while let Some(row)=rows.next()?{
  check_cancel(cancel)?;
  let before:String=row.get(2)?;let after:String=row.get(3)?;
  if before.len()+after.len()>16*1024*1024{return Err(Error::Limit)}
  let _:Value=decode(&before)?;let _:Value=decode(&after)?;
  tx.execute("INSERT INTO legacy_history VALUES(?1,?2,?3,?4,?5,?6)",params![l,row.get::<_,i64>(0)?,row.get::<_,String>(1)?,before,after,row.get::<_,bool>(4)?])?;history+=1;
 }
 let mut q=legacy.prepare("SELECT command_id FROM ovg_edit_command")?;
 let mut rows=q.query([])?;
 while let Some(row)=rows.next()?{
  check_cancel(cancel)?;
  tx.execute("INSERT INTO legacy_command VALUES(?1,?2)",params![l,row.get::<_,String>(0)?])?;
 }
 tx.execute("INSERT INTO legacy_source VALUES(?1,?2,?3,?4,?5)",params![l,hash,size,live,tombstones])?;
 Ok(Migrated{layer:source.layer,live,tombstones,journal,history,sha256:hash.into()})
}
/// Migrate exact-version, checkpointed snapshots into a new SQLite candidate.
/// Caller must keep legacy source backups. No activation or legacy deletion.
pub fn migrate(sources:&[Source],dest:&Path,cancel:&AtomicBool)->Result<Vec<Migrated>>{
 if sources.is_empty()||sources.len()>33||dest.exists(){return Err(Error::Invalid)}
 let mut ids=HashSet::new();
 if sources.iter().any(|s|s.layer.is_nil()||!ids.insert(s.layer)||s.name.trim().is_empty()||s.name.len()>256){return Err(Error::Invalid)}
 let stage=tempfile::tempdir_in(dest.parent().ok_or(Error::Invalid)?).map_err(|_|Error::Storage)?;
 let oracle=stage.path().join("oracle.gpkg");
 {let store=ovg_local_storage::LocalStore::open(&oracle,MAX_SOURCE).map_err(|_|Error::Storage)?;store.checkpoint().map_err(|_|Error::Storage)?;}
 let signature=schema_signature(&open_read(&oracle)?)?;
 let candidate=stage.path().join("candidate.sqlite");
 let s=Store::create(&candidate)?;let mut reports=Vec::new();
 {
  let mut db=s.db.lock().map_err(lock_err)?;let tx=db.transaction_with_behavior(TransactionBehavior::Immediate)?;
  for (i,src) in sources.iter().enumerate(){
   let copy=stage.path().join(format!("source-{i}.gpkg"));
   let (size,hash)=copy_source(&src.snapshot,&copy,cancel)?;
   let legacy=open_read(&copy)?;
   let app:i64=legacy.pragma_query_value(None,"application_id",|r|r.get(0))?;
   let version:i64=legacy.pragma_query_value(None,"user_version",|r|r.get(0))?;
   if app!=0x47504b47||version!=10300||schema_signature(&legacy)?!=signature{return Err(Error::Unsupported)}
   let schema:i64=legacy.query_row("SELECT schema_version FROM ovg_local_metadata WHERE singleton=1",[],|r|r.get(0))?;
   if schema!=3{return Err(Error::Unsupported)}
   legacy.pragma_update(None,"trusted_schema","ON")?;
   let check:String=legacy.pragma_query_value(None,"quick_check",|r|r.get(0))?;
   let fk:i64=legacy.query_row("SELECT count(*) FROM pragma_foreign_key_check",[],|r|r.get(0))?;
   if check!="ok"||fk!=0{return Err(Error::Invalid)}
   reports.push(copy_layer(&tx,src,&legacy,&hash,size,cancel)?);
   drop(legacy);fs::remove_file(copy).map_err(|_|Error::Storage)?;
  }
  tx.execute("UPDATE project SET revision=1 WHERE singleton=1",[])?;
  check_cancel(cancel)?;tx.commit()?;
 }
 if !s.integrity()?{return Err(Error::Invalid)}s.checkpoint()?;drop(s);
 check_cancel(cancel)?;
 fs::File::open(&candidate).and_then(|f|f.sync_all()).map_err(|_|Error::Storage)?;
 fs::hard_link(&candidate,dest).map_err(|_|Error::Storage)?;
 Ok(reports)
}
