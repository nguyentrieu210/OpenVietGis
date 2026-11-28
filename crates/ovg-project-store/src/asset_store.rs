//! Portable project-owned assets. Data chunks live in the same SQLite transaction
//! domain as vectors, so native backup/restore includes them automatically.
use super::*;
use std::io::{Read,Write};
use std::sync::atomic::{AtomicBool,Ordering};
const CHUNK:usize=1024*1024;
const MAX:u64=512*1024*1024;
const TOTAL:u64=2*1024*1024*1024;
#[derive(Clone,Debug,Serialize,Deserialize,PartialEq,Eq)]
pub struct AssetMeta{pub id:Uuid,pub kind:String,pub epsg:Option<u32>,pub bytes:u64,pub sha256:String}
fn profile(kind:&str,epsg:Option<u32>)->bool{match kind{
 "cad"=>epsg==Some(4326),"las"|"laz"|"copc"=>epsg.is_some_and(|e|e>0),"glb"|"mbtiles"|"tiff"=>epsg.is_none(),_=>false
}}
fn cancelled(c:&AtomicBool)->Result<()>{if c.load(Ordering::Acquire){Err(Error::Conflict)}else{Ok(())}}
impl Store{
 pub fn assets(&self)->Result<Vec<AssetMeta>>{
  let db=self.db.lock().map_err(lock_err)?;
  let mut q=db.prepare("SELECT id,kind,epsg,bytes,sha256 FROM project_asset ORDER BY id LIMIT 33")?;
  let rows=q.query_map([],|r|Ok((r.get::<_,String>(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?)))?;
  let mut out=Vec::new();for r in rows{let (id,kind,epsg,bytes,sha256)=r?;out.push(AssetMeta{id:Uuid::parse_str(&id).map_err(|_|Error::Invalid)?,kind,epsg,bytes,sha256})}
  if out.len()>32{return Err(Error::Limit)}Ok(out)
 }
 /// Caller supplies an already validated private snapshot, never an external live file.
 pub fn attach_asset(&self,id:Uuid,kind:&str,epsg:Option<u32>,path:&Path,expected:u64,cancel:&AtomicBool)->Result<AssetMeta>{
  if id.is_nil()||!profile(kind,epsg){return Err(Error::Invalid)}
  let meta=std::fs::symlink_metadata(path).map_err(|_|Error::Storage)?;
  if !meta.file_type().is_file()||meta.len()==0||meta.len()>MAX{return Err(Error::Limit)}
  let mut input=std::fs::File::open(path).map_err(|_|Error::Storage)?;
  let mut db=self.db.lock().map_err(lock_err)?;let tx=db.transaction_with_behavior(TransactionBehavior::Immediate)?;
  let (count,used):(u64,u64)=tx.query_row("SELECT count(*),coalesce(sum(bytes),0) FROM project_asset",[],|r|Ok((r.get(0)?,r.get(1)?)))?;
  if count>=32||used.saturating_add(meta.len())>TOTAL{return Err(Error::Limit)}
  next(&tx,expected)?;
  let key=id.to_string();
  tx.execute("INSERT INTO project_asset VALUES(?1,?2,?3,?4,?5)",params![key,kind,epsg,meta.len(),"0".repeat(64)])?;
  let mut hash=Sha256::new();let mut size=0u64;let mut seq=0u32;let mut buf=vec![0u8;CHUNK];
  loop{
   cancelled(cancel)?;let mut n=0;
   while n<CHUNK{let k=input.read(&mut buf[n..]).map_err(|_|Error::Storage)?;if k==0{break}n+=k;}
   if n==0{break}
   size+=n as u64;if size>meta.len(){return Err(Error::Conflict)}
   hash.update(&buf[..n]);tx.execute("INSERT INTO project_asset_chunk VALUES(?1,?2,?3)",params![key,seq,&buf[..n]])?;seq+=1;
  }
  if size!=meta.len(){return Err(Error::Conflict)}
  let sha256=format!("{:x}",hash.finalize());
  tx.execute("UPDATE project_asset SET sha256=?1 WHERE id=?2",params![sha256,key])?;
  cancelled(cancel)?;tx.commit()?;
  Ok(AssetMeta{id,kind:kind.into(),epsg,bytes:size,sha256})
 }
 pub fn materialize_asset(&self,id:Uuid,dest:&Path,cancel:&AtomicBool)->Result<AssetMeta>{
  let db=self.db.lock().map_err(lock_err)?;
  let key=id.to_string();
  let (kind,epsg,bytes,sha256):(String,Option<u32>,u64,String)=db.query_row("SELECT kind,epsg,bytes,sha256 FROM project_asset WHERE id=?1",[&key],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?)))?;
  if bytes==0||bytes>MAX||!profile(&kind,epsg){return Err(Error::Invalid)}
  let mut tmp=tempfile::NamedTempFile::new_in(dest.parent().ok_or(Error::Invalid)?).map_err(|_|Error::Storage)?;
  let mut hash=Sha256::new();let mut total=0u64;let mut seq=0u32;
  let mut q=db.prepare("SELECT seq,data FROM project_asset_chunk WHERE asset=?1 ORDER BY seq")?;
  let mut rows=q.query([key])?;
  while let Some(r)=rows.next()?{
   cancelled(cancel)?;let n:u32=r.get(0)?;let b:Vec<u8>=r.get(1)?;
   if n!=seq||b.is_empty()||b.len()>CHUNK||total%CHUNK as u64!=0{return Err(Error::Invalid)}
   total+=b.len()as u64;if total>bytes{return Err(Error::Invalid)}
   hash.update(&b);tmp.write_all(&b).map_err(|_|Error::Storage)?;seq+=1;
  }
  if total!=bytes||format!("{:x}",hash.finalize())!=sha256{return Err(Error::Invalid)}
  cancelled(cancel)?;tmp.as_file().sync_all().map_err(|_|Error::Storage)?;
  tmp.persist_noclobber(dest).map_err(|_|Error::Storage)?;
  Ok(AssetMeta{id,kind,epsg,bytes,sha256})
 }
 pub fn detach_asset(&self,id:Uuid,expected:u64)->Result<u64>{
  let mut db=self.db.lock().map_err(lock_err)?;let tx=db.transaction_with_behavior(TransactionBehavior::Immediate)?;
  let n=next(&tx,expected)?;
  if tx.execute("DELETE FROM project_asset WHERE id=?1",[id.to_string()])?!=1{return Err(Error::Invalid)}
  tx.commit()?;Ok(n)
 }
}

pub(crate) fn verify_assets(db:&Connection)->Result<bool>{
 let mut q=db.prepare("SELECT id,kind,epsg,bytes,sha256 FROM project_asset ORDER BY id LIMIT 33")?;
 let mut rows=q.query([])?;let mut count=0;let mut all=0u64;
 while let Some(r)=rows.next()?{
  count+=1;let id:String=r.get(0)?;let kind:String=r.get(1)?;let epsg:Option<u32>=r.get(2)?;let size:u64=r.get(3)?;let digest:String=r.get(4)?;
  all=all.saturating_add(size);
  if count>32||all>TOTAL||size==0||size>MAX||!profile(&kind,epsg)||Uuid::parse_str(&id).ok().is_none_or(|u|u.is_nil()||u.to_string()!=id){return Ok(false)}
  let mut chunks=db.prepare("SELECT seq,data FROM project_asset_chunk WHERE asset=?1 ORDER BY seq")?;
  let mut chunks=chunks.query([id])?;let mut seq=0u32;let mut bytes=0u64;let mut hash=Sha256::new();
  while let Some(c)=chunks.next()?{
   let n:u32=c.get(0)?;let data:Vec<u8>=c.get(1)?;
   if seq!=n||data.is_empty()||data.len()>CHUNK||bytes%CHUNK as u64!=0{return Ok(false)}
   bytes+=data.len()as u64;if bytes>size{return Ok(false)}
   hash.update(&data);seq+=1;
  }
  if bytes!=size||format!("{:x}",hash.finalize())!=digest{return Ok(false)}
 }Ok(true)
}
