//! Separate encrypted transaction domain. No automatic retry on ambiguous receipts.
use super::{Scope,Error,hash,rev,feature_id,lease::{Lease,Clock},sync::{Bootstrap,Delta,Commit,Receipt}};
use rusqlite::{Connection,params,OptionalExtension,TransactionBehavior};
use zeroize::Zeroizing;
use std::path::Path;
use uuid::Uuid;
const MAX:usize=16*1024*1024;
pub struct Store{db:Option<Connection>,scope:Scope,lease:Lease,locked:bool}
impl Store{
 pub fn open(path:&Path,key:Zeroizing<[u8;32]>,scope:Scope,lease:Lease,clock:&Clock)->Result<Self,Error>{
  lease.check(clock,&scope,lease.claims().permission_epoch,false)?;
  if !path.is_absolute()||key.iter().all(|b|*b==0)||std::fs::symlink_metadata(path).is_ok_and(|m|!m.file_type().is_file()){return Err(Error::Invalid);}
  let mut db=Connection::open(path)?;
  // Raw key hex, zeroized on drop. Never interpolate caller text into a PRAGMA.
  let mut raw=Zeroizing::new(String::with_capacity(67));raw.push_str("x'");
  for b in key.iter(){use std::fmt::Write;write!(&mut *raw,"{b:02x}").map_err(|_|Error::Invalid)?;}raw.push('\'');
  db.pragma_update(None,"key",raw.as_str())?;
  let cipher:String=db.query_row("PRAGMA cipher_version",[],|r|r.get(0)).map_err(|_|Error::Locked)?;
  if cipher.is_empty(){return Err(Error::Locked);}
  db.pragma_update(None,"cipher_memory_security","ON")?;
  db.pragma_update(None,"temp_store","MEMORY")?;
  db.pragma_update(None,"foreign_keys","ON")?;
  let count:i64=db.query_row("SELECT count(*) FROM sqlite_master WHERE type='table'",[],|r|r.get(0)).map_err(|_|Error::Locked)?;
  if count==0{
   let tx=db.transaction_with_behavior(TransactionBehavior::Immediate)?;
   tx.execute_batch("CREATE TABLE meta(id INTEGER PRIMARY KEY CHECK(id=1),scope BLOB NOT NULL,epoch INTEGER NOT NULL,cursor INTEGER NOT NULL,package_digest TEXT NOT NULL,ready INTEGER NOT NULL); CREATE TABLE layers(layer TEXT PRIMARY KEY,source TEXT NOT NULL,content TEXT NOT NULL,schema TEXT NOT NULL); CREATE TABLE features(layer TEXT NOT NULL REFERENCES layers(layer),id TEXT NOT NULL,body BLOB NOT NULL,PRIMARY KEY(layer,id)); CREATE TABLE outbox(id TEXT PRIMARY KEY,source TEXT NOT NULL,payload BLOB NOT NULL,digest BLOB NOT NULL,state TEXT NOT NULL,receipt BLOB); CREATE TABLE delta_pages(start INTEGER PRIMARY KEY,end INTEGER NOT NULL,digest BLOB NOT NULL);")?;
   tx.execute("INSERT INTO meta VALUES(1,?1,?2,0,?3,0)",params![scope.digest()?.as_slice(),lease.claims().permission_epoch,lease.claims().package_digest])?;tx.commit()?;
  }
  let (stored,epoch,digest):(Vec<u8>,i64,String)=db.query_row("SELECT scope,epoch,package_digest FROM meta WHERE id=1",[],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?)))?;
  if stored!=scope.digest()?||epoch!=lease.claims().permission_epoch||digest!=lease.claims().package_digest{return Err(Error::Scope);}
  db.pragma_update(None,"journal_mode","WAL")?;db.pragma_update(None,"synchronous","FULL")?;
  db.pragma_update(None,"max_page_count",131072)?;
  db.execute("UPDATE outbox SET state='unknown' WHERE state='sending'",[])?;
  Ok(Self{db:Some(db),scope,lease,locked:false})
 }
 fn guard(&mut self,clock:&Clock,edit:bool)->Result<(),Error>{
  if self.locked{return Err(Error::Locked);}
  if self.lease.check(clock,&self.scope,self.lease.claims().permission_epoch,edit).is_err(){self.revoke();return Err(Error::Locked);}Ok(())
 }
 pub fn lock(mut self)->Result<(),Error>{self.locked=true;match self.db.take(){Some(db)=>db.close().map_err(|(_,e)|Error::Db(e)),None=>Ok(())}}
 /// Candidate signed snapshot payload, exact bytes hash bound by verified lease.
 /// Complete bounded package only; partial pages never become ready.
 pub fn install_snapshot(&mut self,clock:&Clock,bytes:&[u8])->Result<(),Error>{
  self.guard(clock,false)?;if bytes.len()>MAX{return Err(Error::Limit);}
  let hex=hash(bytes).iter().map(|b|format!("{b:02x}")).collect::<String>();
  if hex!=self.lease.claims().package_digest{return Err(Error::Locked);}
  let pages:Vec<Bootstrap>=serde_json::from_slice(bytes).map_err(|_|Error::Invalid)?;
  if pages.is_empty()||pages.len()>128{return Err(Error::Invalid);}
  let cursor=pages[0].cursor;let epoch=self.lease.claims().permission_epoch;
  let tx=self.db.as_mut().ok_or(Error::Locked)?.transaction_with_behavior(TransactionBehavior::Immediate)?;
  let (ready,old_cursor):(bool,i64)=tx.query_row("SELECT ready,cursor FROM meta WHERE id=1",[],|r|Ok((r.get(0)?,r.get(1)?)))?;
  if ready&&old_cursor!=cursor{return Err(Error::Conflict);}
  let pending:i64=tx.query_row("SELECT count(*) FROM outbox WHERE state!='acknowledged'",[],|r|r.get(0))?;
  if pending!=0{return Err(Error::Conflict);}
  tx.execute_batch("DELETE FROM features; DELETE FROM layers; DELETE FROM delta_pages;")?;
  for p in pages{
   if p.has_more||p.next_offset.is_some()||!p.cacheable||p.cursor!=cursor||cursor<0||p.permission_epoch!=epoch||!rev(&p.content_revision)||!rev(&p.schema_revision)||p.layer_id.is_nil()||p.source_id.is_nil()||p.features.len()>5000{return Err(Error::Invalid);}
   let l=p.layer_id.to_string();tx.execute("INSERT INTO layers VALUES(?1,?2,?3,?4)",params![l,p.source_id.to_string(),p.content_revision,p.schema_revision])?;
   for f in p.features{if !f.valid(){return Err(Error::Invalid);}tx.execute("INSERT INTO features VALUES(?1,?2,?3)",params![l,f.feature_id,serde_json::to_vec(&f).map_err(|_|Error::Invalid)?])?;}
  }
  tx.execute("UPDATE meta SET cursor=?1,ready=1 WHERE id=1",[cursor])?;tx.commit()?;Ok(())
 }
 pub fn feature(&mut self,clock:&Clock,layer:Uuid,id:&str)->Result<Option<super::sync::Feature>,Error>{
  self.guard(clock,false)?;if !feature_id(id){return Err(Error::Invalid);}
  let b:Option<Vec<u8>>=self.db.as_ref().ok_or(Error::Locked)?.query_row("SELECT body FROM features WHERE layer=?1 AND id=?2 AND EXISTS(SELECT 1 FROM meta WHERE ready=1)",params![layer.to_string(),id],|r|r.get(0)).optional()?;
  b.map(|b|serde_json::from_slice(&b).map_err(|_|Error::Invalid)).transpose()
 }
 pub fn cursor(&mut self,clock:&Clock)->Result<i64,Error>{self.guard(clock,false)?;Ok(self.db.as_ref().ok_or(Error::Locked)?.query_row("SELECT cursor FROM meta WHERE id=1 AND ready=1",[],|r|r.get(0)).map_err(|_|Error::Reset)?)}
 pub fn apply_delta(&mut self,clock:&Clock,start:i64,bytes:&[u8])->Result<(),Error>{
  self.guard(clock,false)?;if bytes.len()>MAX{return Err(Error::Limit);}
  let d:Delta=serde_json::from_slice(bytes).map_err(|_|Error::Invalid)?;
  if d.reset_required||d.permission_epoch!=self.lease.claims().permission_epoch{self.revoke();return Err(Error::Reset);}
  if start<0||d.next_cursor<start||d.changes.len()>500||d.has_more&&d.next_cursor==start{return Err(Error::Invalid);}
  let current=self.cursor(clock)?;
  let tx=self.db.as_mut().ok_or(Error::Locked)?.transaction_with_behavior(TransactionBehavior::Immediate)?;
  if current!=start{
   let old:Option<(i64,Vec<u8>)>=tx.query_row("SELECT end,digest FROM delta_pages WHERE start=?1",[start],|r|Ok((r.get(0)?,r.get(1)?))).optional()?;
   if old.is_some_and(|(end,h)|end==d.next_cursor&&h==hash(bytes)&&end<=current){return Ok(());}return Err(Error::Reset);
  }
  let mut previous=start;let mut seen=std::collections::HashSet::new();let mut count=0;
  for c in d.changes{
   if c.cursor<previous||c.cursor<=start||c.cursor>d.next_cursor||!rev(&c.content_revision)||!seen.insert((c.cursor,c.layer_id)){return Err(Error::Invalid);}previous=c.cursor;
   let layer=c.layer_id.to_string();let source:Option<String>=tx.query_row("SELECT source FROM layers WHERE layer=?1",[&layer],|r|r.get(0)).optional()?;
   if source.as_deref()!=Some(c.source_id.to_string().as_str()){return Err(Error::Reset);}
   count+=c.upserts.len()+c.removals.len();if count>5000{return Err(Error::Limit);}
   let mut ids=std::collections::HashSet::new();
   for f in c.upserts{if !f.valid()||!ids.insert(f.feature_id.clone()){return Err(Error::Invalid);}
    tx.execute("INSERT INTO features VALUES(?1,?2,?3) ON CONFLICT(layer,id) DO UPDATE SET body=excluded.body",params![layer,f.feature_id,serde_json::to_vec(&f).map_err(|_|Error::Invalid)?])?;}
   for id in c.removals{if !feature_id(&id)||!ids.insert(id.clone()){return Err(Error::Invalid);}tx.execute("DELETE FROM features WHERE layer=?1 AND id=?2",params![layer,id])?;}
   tx.execute("UPDATE layers SET content=?1 WHERE layer=?2",params![c.content_revision,layer])?;
   tx.execute("UPDATE outbox SET state='conflict' WHERE source=?1 AND state='queued'",[c.source_id.to_string()])?;
  }
  if d.next_cursor>start{tx.execute("INSERT INTO delta_pages VALUES(?1,?2,?3)",params![start,d.next_cursor,hash(bytes).as_slice()])?;}
  tx.execute("UPDATE meta SET cursor=?1 WHERE id=1",[d.next_cursor])?;tx.commit()?;Ok(())
 }
 pub fn enqueue(&mut self,clock:&Clock,commit:&Commit)->Result<(),Error>{
  self.guard(clock,true)?;commit.validate()?;
  if commit.session_id!=self.scope.session{return Err(Error::Scope);}
  let data=serde_json::to_vec(commit).map_err(|_|Error::Invalid)?;if data.len()>MAX{return Err(Error::Limit);}
  let tx=self.db.as_mut().ok_or(Error::Locked)?.transaction_with_behavior(TransactionBehavior::Immediate)?;
  let valid:i64=tx.query_row("SELECT count(*) FROM layers WHERE source=?1 AND content=?2 AND schema=?3",params![commit.source_id.to_string(),commit.base_content_revision,commit.base_schema_revision],|r|r.get(0))?;
  if valid==0{return Err(Error::Conflict);}
  let old:Option<Vec<u8>>=tx.query_row("SELECT digest FROM outbox WHERE id=?1",[&commit.operation_id],|r|r.get(0)).optional()?;
  if let Some(h)=old{if h==hash(&data){return Ok(());}return Err(Error::Conflict);}
  tx.execute("INSERT INTO outbox VALUES(?1,?2,?3,?4,'queued',NULL)",params![commit.operation_id,commit.source_id.to_string(),data,hash(&data).as_slice()])?;tx.commit()?;Ok(())
 }
 /// Mark sending before returning bytes. On process death open() changes sending to unknown.
 pub fn begin_send(&mut self,clock:&Clock,id:&str)->Result<Vec<u8>,Error>{
  self.guard(clock,true)?;let tx=self.db.as_mut().ok_or(Error::Locked)?.transaction_with_behavior(TransactionBehavior::Immediate)?;
  let data:Vec<u8>=tx.query_row("SELECT payload FROM outbox WHERE id=?1 AND state='queued'",[id],|r|r.get(0)).optional()?.ok_or(Error::Unknown)?;
  tx.execute("UPDATE outbox SET state='sending' WHERE id=?1",[id])?;tx.commit()?;Ok(data)
 }
 pub fn revoke(&mut self){self.locked=true;drop(self.db.take());}
 pub fn state(&mut self,clock:&Clock,id:&str)->Result<String,Error>{
  self.guard(clock,false)?;Ok(self.db.as_ref().ok_or(Error::Locked)?.query_row("SELECT state FROM outbox WHERE id=?1",[id],|r|r.get(0))?)
 }
 pub fn conflict(&mut self,clock:&Clock,id:&str)->Result<(),Error>{
  self.guard(clock,false)?;
  if self.db.as_ref().ok_or(Error::Locked)?.execute("UPDATE outbox SET state='conflict' WHERE id=?1 AND state='sending'",[id])?!=1{return Err(Error::Conflict);}Ok(())
 }
 pub fn unknown(&mut self,clock:&Clock,id:&str)->Result<(),Error>{self.guard(clock,false)?;if self.db.as_ref().ok_or(Error::Locked)?.execute("UPDATE outbox SET state='unknown' WHERE id=?1 AND state='sending'",[id])?!=1{return Err(Error::Conflict);}Ok(())}
 pub fn receipt(&mut self,clock:&Clock,r:&Receipt)->Result<(),Error>{
  self.guard(clock,false)?;if r.status!="accepted"||!rev(&r.resulting_content_revision)||r.event_cursor<0||r.changeset_id.is_nil(){return Err(Error::Invalid);}
  let tx=self.db.as_mut().ok_or(Error::Locked)?.transaction_with_behavior(TransactionBehavior::Immediate)?;
  let (payload,state,old):(Vec<u8>,String,Option<Vec<u8>>)=tx.query_row("SELECT payload,state,receipt FROM outbox WHERE id=?1",[&r.operation_id],|row|Ok((row.get(0)?,row.get(1)?,row.get(2)?)))?;
  let c:Commit=serde_json::from_slice(&payload).map_err(|_|Error::Invalid)?;
  if c.source_id!=r.source_id||c.operations.len()!=r.operation_count as usize{return Err(Error::Conflict);}
  let data=serde_json::to_vec(r).map_err(|_|Error::Invalid)?;
  if state=="acknowledged"{return if old==Some(data){Ok(())}else{Err(Error::Conflict)};}
  if state!="sending"&&state!="unknown"{return Err(Error::Conflict);}
  tx.execute("UPDATE outbox SET state='acknowledged',receipt=?1 WHERE id=?2",params![data,r.operation_id])?;
  // A receipt is not a delta: do not advance cursor or replace base geometry.
  tx.commit()?;Ok(())
 }
}
