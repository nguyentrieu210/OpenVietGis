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
