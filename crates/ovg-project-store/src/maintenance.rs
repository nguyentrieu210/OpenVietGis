//! Explicit maintenance; never silently repair or replace the authoritative file.
use super::*;
use std::sync::atomic::{AtomicBool,Ordering};
#[derive(Debug,Serialize)]
pub struct Report{pub revision:u64,pub features:u64,pub tombstones:u64,pub pages:u64,pub free_pages:u64,pub missing:u64,pub orphan:u64,pub foreign_keys:u64,pub integrity:String}
impl Store{
 pub fn report(&self)->Result<Report>{
  let db=self.db.lock().map_err(lock_err)?;
  let count=|s:&str|->Result<u64>{Ok(db.query_row(s,[],|r|r.get(0))?)};
  Ok(Report{revision:rev(&db)?,features:count("SELECT count(*) FROM feature")?,tombstones:count("SELECT count(*) FROM feature_revision r LEFT JOIN feature f ON f.layer=r.layer AND f.id=r.id WHERE f.fid IS NULL")?,
   pages:db.pragma_query_value(None,"page_count",|r|r.get(0))?,free_pages:db.pragma_query_value(None,"freelist_count",|r|r.get(0))?,
   missing:count("SELECT count(*) FROM feature f LEFT JOIN feature_index i ON i.fid=f.fid WHERE i.fid IS NULL OR i.minx>f.minx OR i.maxx<f.maxx OR i.miny>f.miny OR i.maxy<f.maxy")?,
   orphan:count("SELECT count(*) FROM feature_index i LEFT JOIN feature f ON f.fid=i.fid WHERE f.fid IS NULL")?,
   foreign_keys:count("SELECT count(*) FROM pragma_foreign_key_check")?,integrity:db.pragma_query_value(None,"quick_check",|r|r.get(0))?})
 }
 pub fn checkpoint(&self)->Result<()>{
  let db=self.db.lock().map_err(lock_err)?;
  let busy:i64=db.query_row("PRAGMA wal_checkpoint(TRUNCATE)",[],|r|r.get(0))?;
  if busy!=0{return Err(Error::Storage)}Ok(())
 }
 /// Caller supplies a verified backup. Validate geometry before committing a rebuilt index.
 pub fn rebuild_index(&self,expected:u64,cancel:&AtomicBool)->Result<u64>{
  let mut db=self.db.lock().map_err(lock_err)?;let tx=db.transaction_with_behavior(TransactionBehavior::Immediate)?;
  if rev(&tx)?!=expected{return Err(Error::Conflict)}
  let mut after=0i64;let mut count=0u64;
  loop{
   if cancel.load(Ordering::Acquire){return Err(Error::Conflict)}
   let page={
    let mut q=tx.prepare("SELECT fid,layer,wkb,attributes FROM feature WHERE fid>?1 ORDER BY fid LIMIT 32")?;
    let rows=q.query_map([after],|r|Ok((r.get::<_,i64>(0)?,r.get::<_,String>(1)?,r.get::<_,Vec<u8>>(2)?,r.get::<_,String>(3)?)))?;
    rows.collect::<rusqlite::Result<Vec<_>>>()?
   };
   if page.is_empty(){break}
   for (fid,l,w,a) in page{
    if cancel.load(Ordering::Acquire){return Err(Error::Conflict)}
    let l=Uuid::parse_str(&l).map_err(|_|Error::Invalid)?;
    let (b,_)=validate_image(&tx,l,&Image{wkb:w,attributes:decode(&a)?})?;
    tx.execute("UPDATE feature SET minx=?1,maxx=?2,miny=?3,maxy=?4 WHERE fid=?5",params![b[0],b[1],b[2],b[3],fid])?;
    after=fid;count+=1;
   }
  }
  tx.execute("DELETE FROM feature_index",[])?;
  tx.execute("INSERT INTO feature_index SELECT fid,minx,maxx,miny,maxy FROM feature",[])?;
  if cancel.load(Ordering::Acquire){return Err(Error::Conflict)}
  tx.commit()?;Ok(count)
 }
 /// Copy first, then repair the derived index. No change to the input database.
 pub fn rebuilt_copy(&self,dest:&Path,expected:u64,cancel:&AtomicBool)->Result<()>{
  if dest.exists(){return Err(Error::Invalid)}
  let stage=tempfile::tempdir_in(dest.parent().ok_or(Error::Invalid)?).map_err(|_|Error::Storage)?;
  let candidate=stage.path().join("rebuild.sqlite");
  {
   let db=self.db.lock().map_err(lock_err)?;
   if rev(&db)?!=expected{return Err(Error::Conflict)}
   let mut copy=Connection::open(&candidate)?;
   let backup=rusqlite::backup::Backup::new(&db,&mut copy)?;
   let deadline=std::time::Instant::now()+std::time::Duration::from_secs(120);
   loop{
    if cancel.load(Ordering::Acquire)||std::time::Instant::now()>deadline{return Err(Error::Conflict)}
    match backup.step(128)?{rusqlite::backup::StepResult::Done=>break,_=>std::thread::sleep(std::time::Duration::from_millis(2))}
   }
  }
  {let verified=Store::reopen(&candidate)?;verified.rebuild_index(expected,cancel)?;
   if !verified.integrity()?{return Err(Error::Invalid)}verified.checkpoint()?;}
  if cancel.load(Ordering::Acquire){return Err(Error::Conflict)}
  std::fs::File::open(&candidate).and_then(|f|f.sync_all()).map_err(|_|Error::Storage)?;
  std::fs::hard_link(&candidate,dest).map_err(|_|Error::Storage)?;Ok(())
 }
 /// Compact into a verified new file. The source remains untouched.
 pub fn compact_copy(&self,dest:&Path,expected:u64)->Result<()>{
  let parent=dest.parent().ok_or(Error::Invalid)?;
  let stage=tempfile::tempdir_in(parent).map_err(|_|Error::Storage)?;
  let candidate=stage.path().join("compact.sqlite");
  {
   let db=self.db.lock().map_err(lock_err)?;
   if rev(&db)?!=expected{return Err(Error::Conflict)}
   db.execute("VACUUM INTO ?1",[candidate.to_str().ok_or(Error::Invalid)?])?;
  }
  {let verified=Store::reopen(&candidate)?;if verified.revision()?!=expected||!verified.integrity()?{return Err(Error::Invalid)}verified.checkpoint()?;}
  std::fs::File::open(&candidate).and_then(|f|f.sync_all()).map_err(|_|Error::Storage)?;
  std::fs::hard_link(&candidate,dest).map_err(|_|Error::Storage)?;Ok(())
 }
}

impl Store{
 /// Backup a coherent revision, including all receipts, relationships and journals.
 pub fn backup_copy(&self,dest:&Path,expected:u64,cancel:&AtomicBool)->Result<()>{
  if dest.exists(){return Err(Error::Invalid)}
  let stage=tempfile::tempdir_in(dest.parent().ok_or(Error::Invalid)?).map_err(|_|Error::Storage)?;
  let p=stage.path().join("backup.sqlite");
  {let db=self.db.lock().map_err(lock_err)?;
   if rev(&db)?!=expected{return Err(Error::Conflict)}
   let mut target=Connection::open(&p)?;
   let backup=rusqlite::backup::Backup::new(&db,&mut target)?;
   let deadline=std::time::Instant::now()+std::time::Duration::from_secs(120);
   loop{if cancel.load(Ordering::Acquire)||std::time::Instant::now()>deadline{return Err(Error::Conflict)}
    match backup.step(256)?{rusqlite::backup::StepResult::Done=>break,_=>std::thread::sleep(std::time::Duration::from_millis(2))}
   }
  }
  {let s=Store::reopen(&p)?;if s.revision()?!=expected||!s.integrity()?{return Err(Error::Invalid)}s.checkpoint()?;}
  if cancel.load(Ordering::Acquire){return Err(Error::Conflict)}
  std::fs::File::open(&p).and_then(|f|f.sync_all()).map_err(|_|Error::Storage)?;
  std::fs::hard_link(&p,dest).map_err(|_|Error::Storage)?;Ok(())
 }
 /// Import only a checkpointed, exact-schema backup into a new file.
 /// Never open the external file with write privileges.
 pub fn restore_copy(source:&Path,dest:&Path,cancel:&AtomicBool)->Result<u64>{
  use std::io::{Read,Write};
  if dest.exists(){return Err(Error::Invalid)}
  let meta=std::fs::symlink_metadata(source).map_err(|_|Error::Storage)?;
  if !meta.file_type().is_file()||meta.len()>QUOTA{return Err(Error::Limit)}
  for suffix in ["-wal","-shm","-journal"]{let mut n=source.as_os_str().to_os_string();n.push(suffix);
   match std::fs::symlink_metadata(n){Err(e) if e.kind()==std::io::ErrorKind::NotFound=>{},_=>return Err(Error::Unsupported)}
  }
  let stage=tempfile::tempdir_in(dest.parent().ok_or(Error::Invalid)?).map_err(|_|Error::Storage)?;
  let p=stage.path().join("restore.sqlite");
  let mut from=std::fs::File::open(source).map_err(|_|Error::Storage)?;
  let mut to=std::fs::OpenOptions::new().create_new(true).write(true).open(&p).map_err(|_|Error::Storage)?;
  let mut size=0u64;let mut b=vec![0u8;256*1024];
  loop{if cancel.load(Ordering::Acquire){return Err(Error::Conflict)}
   let n=from.read(&mut b).map_err(|_|Error::Storage)?;if n==0{break}size+=n as u64;
   if size>QUOTA{return Err(Error::Limit)}to.write_all(&b[..n]).map_err(|_|Error::Storage)?;
  }
  let end=std::fs::symlink_metadata(source).map_err(|_|Error::Storage)?;
  if size!=meta.len()||end.len()!=meta.len()||end.modified().ok()!=meta.modified().ok(){return Err(Error::Conflict)}
  to.sync_all().map_err(|_|Error::Storage)?;drop(to);
  let revision={let s=Store::reopen(&p)?;if !s.integrity()?{return Err(Error::Invalid)}s.checkpoint()?;s.revision()?};
  if cancel.load(Ordering::Acquire){return Err(Error::Conflict)}
  std::fs::File::open(&p).and_then(|f|f.sync_all()).map_err(|_|Error::Storage)?;
  std::fs::hard_link(&p,dest).map_err(|_|Error::Storage)?;Ok(revision)
 }
}
