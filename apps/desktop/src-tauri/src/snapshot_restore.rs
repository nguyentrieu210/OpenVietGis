//! Restore a standalone OVG snapshot into a new private project. Never overwrite.
use ovg_local_storage::LocalStore;
use rusqlite::{Connection,OpenFlags};
use std::{fs,io::Read,path::Path};
type Result<T>=std::result::Result<T,&'static str>;
const MAX:u64=512*1024*1024;
fn db(p:&Path)->Result<Connection>{
 let c=Connection::open_with_flags(p,OpenFlags::SQLITE_OPEN_READ_ONLY|OpenFlags::SQLITE_OPEN_NO_MUTEX).map_err(|_|"INVALID_SNAPSHOT")?;
 c.pragma_update(None,"trusted_schema","OFF").map_err(|_|"INVALID_SNAPSHOT")?;
 c.pragma_update(None,"query_only","ON").map_err(|_|"INVALID_SNAPSHOT")?;
 Ok(c)
}
fn schema(c:&Connection)->Result<Vec<(String,String,String)>>{
 let mut q=c.prepare("SELECT type,name,coalesce(sql,'') FROM sqlite_master WHERE name NOT LIKE 'sqlite_%' ORDER BY type,name").map_err(|_|"INVALID_SNAPSHOT")?;
 let rows=q.query_map([],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?))).map_err(|_|"INVALID_SNAPSHOT")?;
 rows.collect::<rusqlite::Result<Vec<_>>>().map_err(|_|"INVALID_SNAPSHOT")
}
pub fn restore(source:&Path,stage:&Path)->Result<()>{
 let m=fs::symlink_metadata(source).map_err(|_|"INVALID_SNAPSHOT")?;
 if !m.file_type().is_file()||m.len()>MAX{return Err("RESOURCE_LIMIT")}
 for suffix in ["-wal","-shm","-journal"]{
   let mut n=source.as_os_str().to_os_string();n.push(suffix);
   match fs::symlink_metadata(n){Err(e) if e.kind()==std::io::ErrorKind::NotFound=>{},_=>return Err("CHECKPOINTED_SNAPSHOT_REQUIRED")}
 }
 let path=stage.join("workspace.gpkg");
 let mut input=fs::File::open(source).map_err(|_|"INVALID_SNAPSHOT")?.take(MAX+1);
 let mut out=fs::OpenOptions::new().write(true).create_new(true).open(&path).map_err(|_|"STORAGE_UNAVAILABLE")?;
 let n=std::io::copy(&mut input,&mut out).map_err(|_|"STORAGE_UNAVAILABLE")?;
 if n!=m.len()||n>MAX{return Err("SOURCE_CHANGED")}
 out.sync_all().map_err(|_|"STORAGE_UNAVAILABLE")?;drop(out);
 // Compare executable schema against an actual fresh trusted shared LocalStore.
 // Current schema only: older formats require a separately qualified migration.
 let oracle=tempfile::tempdir_in(stage).map_err(|_|"STORAGE_UNAVAILABLE")?;
 let op=oracle.path().join("oracle.gpkg");
 {let s=LocalStore::open(&op,MAX).map_err(|_|"STORAGE_UNAVAILABLE")?;s.checkpoint().map_err(|_|"STORAGE_UNAVAILABLE")?;}
 {let a=db(&path)?;let b=db(&op)?;
 let app:i32=a.pragma_query_value(None,"application_id",|r|r.get(0)).map_err(|_|"INVALID_SNAPSHOT")?;
 let version:i64=a.query_row("SELECT schema_version FROM ovg_local_metadata WHERE singleton=1",[],|r|r.get(0)).map_err(|_|"INVALID_SNAPSHOT")?;
 let current:i64=b.query_row("SELECT schema_version FROM ovg_local_metadata WHERE singleton=1",[],|r|r.get(0)).map_err(|_|"INVALID_SNAPSHOT")?;
 if app!=0x47504b47||version!=current||schema(&a)?!=schema(&b)?{return Err("UNSUPPORTED_SNAPSHOT_SCHEMA")}
 let integrity:String=a.pragma_query_value(None,"integrity_check",|r|r.get(0)).map_err(|_|"INVALID_SNAPSHOT")?;
 if integrity!="ok"{return Err("INVALID_SNAPSHOT")}
 let foreign:i64=a.query_row("SELECT count(*) FROM pragma_foreign_key_check",[],|r|r.get(0)).map_err(|_|"INVALID_SNAPSHOT")?;
 if foreign!=0{return Err("INVALID_SNAPSHOT")}
 }
 drop(oracle);
 {let s=LocalStore::open(&path,MAX).map_err(|_|"INVALID_SNAPSHOT")?;s.checkpoint().map_err(|_|"STORAGE_UNAVAILABLE")?;}
 fs::File::open(&path).and_then(|f|f.sync_all()).map_err(|_|"STORAGE_UNAVAILABLE")?;
 Ok(())
}
