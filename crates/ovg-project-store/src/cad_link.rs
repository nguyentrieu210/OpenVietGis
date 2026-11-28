//! Durable CAD layer binding and atomic publication of linked revisions.
use super::*;
use std::sync::atomic::{AtomicBool,Ordering};

#[derive(Clone,Debug,Serialize,Deserialize,PartialEq,Eq)]
pub struct Binding{pub handle:String,pub feature:Uuid}
#[derive(Clone,Debug,Serialize,Deserialize,PartialEq,Eq)]
pub struct CadLink{pub layer:Uuid,pub asset:Uuid,pub root:Uuid,pub automatic:bool,pub bindings:Vec<Binding>}
pub struct CadPublication<'a>{pub id:Uuid,pub path:&'a Path,pub sha:&'a str,pub parent_sha:&'a str}
fn handle(s:&str)->bool{!s.is_empty()&&s.len()<=16&&s.bytes().all(|v|v.is_ascii_hexdigit()&&!v.is_ascii_lowercase())}
fn uuid(s:&str)->Result<Uuid>{Uuid::parse_str(s).ok().filter(|v|!v.is_nil()&&v.to_string()==s).ok_or(Error::Invalid)}
fn read(db:&Connection,layer:Uuid)->Result<Option<CadLink>>{
 let row=db.query_row("SELECT asset,root,automatic FROM cad_link WHERE layer=?1",[layer.to_string()],|r|Ok((r.get::<_,String>(0)?,r.get::<_,String>(1)?,r.get::<_,bool>(2)?))).optional()?;
 let Some((asset,root,automatic))=row else{return Ok(None)};
 let mut q=db.prepare("SELECT handle,feature FROM cad_binding WHERE layer=?1 ORDER BY handle LIMIT 4097")?;
 let mut rows=q.query([layer.to_string()])?;let mut bindings=Vec::new();
 while let Some(r)=rows.next()?{let h:String=r.get(0)?;if !handle(&h)||bindings.len()>=4096{return Err(Error::Limit)}bindings.push(Binding{handle:h,feature:uuid(&r.get::<_,String>(1)?)?});}
 Ok(Some(CadLink{layer,asset:uuid(&asset)?,root:uuid(&root)?,automatic,bindings}))
}
fn root(db:&Connection,asset:Uuid)->Result<Uuid>{
 let mut at=asset;let mut seen=HashSet::new();
 loop{
  if !seen.insert(at)||seen.len()>32{return Err(Error::Invalid)}
  let kind:String=db.query_row("SELECT kind FROM project_asset WHERE id=?1",[at.to_string()],|r|r.get(0))?;
  if kind!="cad-native-dxf"{return Err(Error::Invalid)}
  let parent=db.query_row("SELECT parent FROM cad_version WHERE asset=?1",[at.to_string()],|r|r.get::<_,String>(0)).optional()?;
  if let Some(p)=parent{at=uuid(&p)?}else{return Ok(at)}
 }
}
fn insert_bindings(db:&Connection,layer:Uuid,bindings:&[Binding])->Result<()>{
 if bindings.len()>64{return Err(Error::Limit)}
 let count:u64=db.query_row("SELECT count(*) FROM cad_binding WHERE layer=?1",[layer.to_string()],|r|r.get(0))?;
 if count+bindings.len()as u64>4096{return Err(Error::Limit)}
 for b in bindings{
  if !handle(&b.handle)||b.feature.is_nil(){return Err(Error::Invalid)}
  if current(db,layer,b.feature)?.1.is_none(){return Err(Error::Conflict)}
  db.execute("INSERT INTO cad_binding VALUES(?1,?2,?3)",params![layer.to_string(),b.handle,b.feature.to_string()])?;
 }
 Ok(())
}
impl Store{
 pub fn cad_link(&self,layer:Uuid,expected:u64)->Result<Option<CadLink>>{
  let db=self.db.lock().map_err(lock_err)?;if rev(&db)?!=expected{return Err(Error::Conflict)}known_layer(&db,layer)?;read(&db,layer)
 }
 pub fn register_cad_link(&self,layer:Uuid,asset:Uuid,expected:u64,bindings:&[Binding],automatic:bool)->Result<u64>{
  let mut db=self.db.lock().map_err(lock_err)?;let tx=db.transaction_with_behavior(TransactionBehavior::Immediate)?;
  known_layer(&tx,layer)?;if read(&tx,layer)?.is_some(){return Err(Error::Conflict)}
  let root=root(&tx,asset)?;let n=next(&tx,expected)?;
  tx.execute("INSERT INTO cad_link VALUES(?1,?2,?3,?4)",params![layer.to_string(),asset.to_string(),root.to_string(),automatic])?;
  insert_bindings(&tx,layer,bindings)?;commit_batch(tx)?;Ok(n)
 }
 pub fn cad_link_automatic(&self,layer:Uuid,expected:u64,automatic:bool)->Result<u64>{
  let mut db=self.db.lock().map_err(lock_err)?;let tx=db.transaction_with_behavior(TransactionBehavior::Immediate)?;
  if read(&tx,layer)?.is_none(){return Err(Error::Invalid)}let n=next(&tx,expected)?;
  tx.execute("UPDATE cad_link SET automatic=?1 WHERE layer=?2",params![automatic,layer.to_string()])?;commit_batch(tx)?;Ok(n)
 }
 /// Publishes an existing direct child or a staged new version and all
 /// provenance updates together. Binding records persist through tombstones.
 pub fn commit_cad_link(&self,layer:Uuid,base:Uuid,target:Uuid,publication:Option<CadPublication<'_>>,batch:&Batch,additions:&[Binding],cancel:&AtomicBool)->Result<u64>{
  if target==base||!batch.links.is_empty()||batch.edits.iter().any(|e|e.layer!=layer)||batch.edits.len()>64||batch.operation.is_nil(){return Err(Error::Invalid)}
  let mut db=self.db.lock().map_err(lock_err)?;let tx=db.transaction_with_behavior(TransactionBehavior::Immediate)?;
  if rev(&tx)?!=batch.expected_project||cancel.load(Ordering::Acquire){return Err(Error::Conflict)}
  let link=read(&tx,layer)?.ok_or(Error::Invalid)?;if link.asset!=base{return Err(Error::Conflict)}
  if let Some(p)=publication{
   if p.id!=target||p.sha.len()!=64{return Err(Error::Invalid)}
   Self::attach_tx(&tx,p.id,"cad-native-dxf",None,p.path,cancel,Some(p.sha),Some((base,p.parent_sha)))?;
  }
  let parent:Option<String>=tx.query_row("SELECT parent FROM cad_version WHERE asset=?1",[target.to_string()],|r|r.get(0)).optional()?;
  if parent.as_deref()!=Some(base.to_string().as_str())||root(&tx,target)?!=link.root{return Err(Error::Conflict)}
  let n=if batch.edits.is_empty(){next(&tx,batch.expected_project)?}else{Self::apply_tx(&tx,batch)?.revision};
  insert_bindings(&tx,layer,additions)?;
  tx.execute("UPDATE cad_link SET asset=?1 WHERE layer=?2",params![target.to_string(),layer.to_string()])?;
  tx.execute("INSERT INTO cad_link_publication VALUES(?1,?2,?3,?4)",params![n,layer.to_string(),base.to_string(),target.to_string()])?;
  if cancel.load(Ordering::Acquire){return Err(Error::Conflict)}commit_batch(tx)?;Ok(n)
 }
}
pub(crate) fn verify(db:&Connection)->Result<bool>{
 if db.query_row("SELECT EXISTS(SELECT 1 FROM cad_link_publication h LEFT JOIN cad_version v ON v.asset=h.asset WHERE v.asset IS NULL OR v.parent<>h.previous OR h.revision>(SELECT revision FROM project WHERE singleton=1))",[],|r|r.get::<_,bool>(0))?{return Ok(false)}
 let mut q=db.prepare("SELECT layer FROM cad_link LIMIT 257")?;let mut rows=q.query([])?;let mut count=0;
 while let Some(r)=rows.next()?{count+=1;if count>256{return Ok(false)}
  let l=read(db,uuid(&r.get::<_,String>(0)?)?)?.ok_or(Error::Invalid)?;
  if root(db,l.asset)?!=l.root{return Ok(false)}
 }
 Ok(true)
}
