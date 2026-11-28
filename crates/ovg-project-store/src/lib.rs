//! R3 single-domain transaction candidate; used by opt-in Desktop, not managed data.
//! Do not open legacy GeoPackages with this kernel.
use rusqlite::{params,Connection,OptionalExtension,TransactionBehavior};
use serde::{Deserialize,Serialize};
use serde_json::Value;
use sha2::{Digest,Sha256};
use std::{collections::HashSet,path::Path,sync::Mutex};
use uuid::Uuid;
pub mod catalog;
pub mod mobile;
pub mod mvt;
pub mod tile_archive;
pub mod survey;
pub mod asset_store;
pub mod bulk_import;
pub mod migration;
pub mod maintenance;
pub mod virtual_query;
use ovg_mobile_engine::attributes::{self,Field};
use ovg_spatial::{parse_xy_wkb,validate_geometry,ValidationLimits};
#[derive(Debug,PartialEq,Eq)]
pub enum Error{Invalid,Conflict,Reused,Limit,Storage,Relation,Unsupported}
impl From<rusqlite::Error> for Error{fn from(_:rusqlite::Error)->Self{Error::Storage}}
type Result<T>=std::result::Result<T,Error>;
fn json<T:Serialize>(v:&T)->Result<String>{serde_json::to_string(v).map_err(|_|Error::Invalid)}
fn decode<T:serde::de::DeserializeOwned>(v:&str)->Result<T>{serde_json::from_str(v).map_err(|_|Error::Invalid)}
const APP:i64=0x4f564750;
const VERSION:i64=4;
const V4_MARKER:&str="-- V4 additive survey records";
const V3_MARKER:&str="-- V3 additive assets and bulk import";
const QUOTA:u64=8*1024*1024*1024;
fn configure(db:&Connection)->Result<()>{
 db.busy_timeout(std::time::Duration::from_secs(3))?;
 db.execute_batch("PRAGMA foreign_keys=ON; PRAGMA journal_mode=WAL; PRAGMA synchronous=FULL; PRAGMA wal_autocheckpoint=256; PRAGMA journal_size_limit=8388608;")?;
 let page:u64=db.pragma_query_value(None,"page_size",|r|r.get(0))?;
 let used:u64=db.pragma_query_value(None,"page_count",|r|r.get(0))?;
 if page==0||used>QUOTA/page{return Err(Error::Limit)}
 db.pragma_update(None,"max_page_count",QUOTA/page)?;Ok(())
}
fn schema_signature(db:&Connection)->Result<Vec<(String,String,String)>>{
 let mut q=db.prepare("SELECT type,name,coalesce(sql,'') FROM sqlite_master WHERE name NOT LIKE 'sqlite_%' ORDER BY type,name")?;
 let rows=q.query_map([],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?)))?;
 Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}
const MAX_BATCH:usize=64;
const MAX_BYTES:usize=4*1024*1024;
#[derive(Clone,Debug,Serialize,Deserialize,PartialEq)]
pub struct Image{pub wkb:Vec<u8>,pub attributes:Value}
#[derive(Clone,Debug,Serialize,Deserialize)]
pub struct Edit{pub layer:Uuid,pub feature:Uuid,pub expected:u64,pub image:Option<Image>}
#[derive(Clone,Debug,Serialize,Deserialize)]
pub struct Link{pub relation:Uuid,pub source:Uuid,pub target:Uuid,pub add:bool}
#[derive(Clone,Debug,Serialize,Deserialize)]
pub struct Batch{pub operation:Uuid,pub expected_project:u64,pub edits:Vec<Edit>,pub links:Vec<Link>}
#[derive(Clone,Debug,Serialize,Deserialize,PartialEq,Eq)]
pub struct Receipt{pub revision:u64,pub features:Vec<(Uuid,Uuid,u64)>}
#[derive(Clone,Debug,Serialize,Deserialize)]
pub struct Row{pub fid:i64,pub layer:Uuid,pub id:Uuid,pub revision:u64,pub image:Image}
#[derive(Clone,Debug,Serialize,Deserialize)]
pub struct Page{pub revision:u64,pub rows:Vec<Row>,pub next:Option<i64>}
pub struct Store{db:Mutex<Connection>}
fn lock_err<T>(_:std::sync::PoisonError<T>)->Error{Error::Storage}
fn rev(db:&Connection)->Result<u64>{Ok(db.query_row("SELECT revision FROM project WHERE singleton=1",[],|r|r.get(0))?)}
fn next(db:&Connection,expected:u64)->Result<u64>{
 if rev(db)?!=expected{return Err(Error::Conflict)}
 let n=expected.checked_add(1).filter(|n|*n<=i64::MAX as u64).ok_or(Error::Limit)?;
 db.execute("UPDATE project SET revision=?1 WHERE singleton=1",[n])?;Ok(n)
}
fn known_layer(db:&Connection,id:Uuid)->Result<()>{
 if id.is_nil()||!db.query_row("SELECT EXISTS(SELECT 1 FROM layer WHERE id=?1)",[id.to_string()],|r|r.get::<_,bool>(0))?{return Err(Error::Invalid)}Ok(())
}
fn schema(db:&Connection,id:Uuid)->Result<Option<Vec<Field>>>{
 let v:Option<String>=db.query_row("SELECT schema_json FROM layer WHERE id=?1",[id.to_string()],|r|r.get(0))?;
 v.map(|x|decode(&x)).transpose()
}
fn current(db:&Connection,l:Uuid,id:Uuid)->Result<(u64,Option<Image>)>{
 let rev=db.query_row("SELECT revision FROM feature_revision WHERE layer=?1 AND id=?2",params![l.to_string(),id.to_string()],|r|r.get::<_,u64>(0)).optional()?.unwrap_or(0);
 let img=db.query_row("SELECT wkb,attributes FROM feature WHERE layer=?1 AND id=?2",params![l.to_string(),id.to_string()],|r|Ok((r.get::<_,Vec<u8>>(0)?,r.get::<_,String>(1)?))).optional()?;
 Ok((rev,img.map(|(w,a)|->Result<Image>{Ok(Image{wkb:w,attributes:decode(&a)?})}).transpose()?))
}
fn validate_image(db:&Connection,l:Uuid,img:&Image)->Result<([f64;4],String)>{
 let a=json(&img.attributes)?;
 if img.wkb.len()>1024*1024||a.len()>256*1024||!img.attributes.is_object(){return Err(Error::Limit)}
 if let Some(s)=schema(db,l)?{attributes::validate(&s,&img.attributes).map_err(|_|Error::Invalid)?;}
 let limits=ValidationLimits::default();
 let g=parse_xy_wkb(&img.wkb,&limits).map_err(|_|Error::Invalid)?;
 if g.srid.is_some_and(|s|s!=4326){return Err(Error::Invalid)}
 let b=validate_geometry(&g.geometry,&limits).map_err(|_|Error::Invalid)?.bbox;
 if b.min_x< -180.||b.max_x>180.||b.min_y< -90.||b.max_y>90.{return Err(Error::Invalid)}
 Ok(([b.min_x,b.max_x,b.min_y,b.max_y],a))
}
impl Store{
 pub fn create(path:&Path)->Result<Self>{
  // Exclusive creation; never overwrite a legacy database.
  std::fs::OpenOptions::new().write(true).create_new(true).open(path).map_err(|_|Error::Storage)?;
  let result=(||{
   let mut db=Connection::open(path)?;
   configure(&db)?;
   let tx=db.transaction_with_behavior(TransactionBehavior::Immediate)?;
   tx.execute_batch(include_str!("schema.sql"))?;
   tx.pragma_update(None,"application_id",APP)?;
   tx.pragma_update(None,"user_version",VERSION)?;
   tx.commit()?;
   Ok(Self{db:Mutex::new(db)})
  })();
  if result.is_err(){let _=std::fs::remove_file(path);}result
 }
 pub fn reopen(path:&Path)->Result<Self>{
  if !std::fs::symlink_metadata(path).map_err(|_|Error::Storage)?.file_type().is_file(){return Err(Error::Invalid)}
  let db=Connection::open_with_flags(path,rusqlite::OpenFlags::SQLITE_OPEN_READ_WRITE)?;
  db.pragma_update(None,"trusted_schema","OFF")?;
  let app:i64=db.pragma_query_value(None,"application_id",|r|r.get(0))?;
  let version:i64=db.pragma_query_value(None,"user_version",|r|r.get(0))?;
  if app!=APP||!matches!(version,2|3|VERSION){return Err(Error::Unsupported)}
  let schema=include_str!("schema.sql");
  let (v3,v4)=schema.split_once(V4_MARKER).ok_or(Error::Unsupported)?;
  let (v2,v3_addition)=v3.split_once(V3_MARKER).ok_or(Error::Unsupported)?;
  let oracle=Connection::open_in_memory()?;oracle.execute_batch(match version{2=>v2,3=>v3,_=>schema})?;
  if schema_signature(&db)?!=schema_signature(&oracle)?{return Err(Error::Unsupported)}
  db.pragma_update(None,"trusted_schema","ON")?;
  let check:String=db.pragma_query_value(None,"quick_check",|r|r.get(0))?;
  if check!="ok"{return Err(Error::Invalid)}
  configure(&db)?;
  if version<VERSION{
   let tx=db.unchecked_transaction()?;
   if version==2{tx.execute_batch(v3_addition)?;}
   tx.execute_batch(v4)?;tx.pragma_update(None,"user_version",VERSION)?;tx.commit()?;
  }
  Ok(Self{db:Mutex::new(db)})
 }
 pub fn revision(&self)->Result<u64>{rev(&*self.db.lock().map_err(lock_err)?)}
 pub fn add_layer(&self,id:Uuid,name:&str,expected:u64)->Result<u64>{
  if id.is_nil()||name.trim().is_empty()||name.len()>256{return Err(Error::Invalid)}
  let mut db=self.db.lock().map_err(lock_err)?;let tx=db.transaction_with_behavior(TransactionBehavior::Immediate)?;
  if tx.query_row("SELECT count(*) FROM layer",[],|r|r.get::<_,u64>(0))?>=256{return Err(Error::Limit)}
  let n=next(&tx,expected)?;
  tx.execute("INSERT INTO layer VALUES(?1,?2,NULL)",params![id.to_string(),name])?;
  tx.commit()?;Ok(n)
 }
 pub fn set_schema(&self,id:Uuid,fields:&[Field],expected:u64)->Result<u64>{
  attributes::validate_schema(fields).map_err(|_|Error::Invalid)?;
  let mut db=self.db.lock().map_err(lock_err)?;let tx=db.transaction_with_behavior(TransactionBehavior::Immediate)?;
  known_layer(&tx,id)?;let n=next(&tx,expected)?;
  {let mut q=tx.prepare("SELECT attributes FROM feature WHERE layer=?1")?;let mut rows=q.query([id.to_string()])?;
   while let Some(r)=rows.next()?{let a:Value=decode(&r.get::<_,String>(0)?)?;attributes::validate(fields,&a).map_err(|_|Error::Conflict)?;}}
  tx.execute("UPDATE layer SET schema_json=?1 WHERE id=?2",params![json(&fields)?,id.to_string()])?;tx.commit()?;Ok(n)
 }
 pub fn add_relation(&self,id:Uuid,source:Uuid,target:Uuid,kind:&str,expected:u64)->Result<u64>{
  if id.is_nil()||!matches!(kind,"one_one"|"one_many"|"many_many"){return Err(Error::Invalid)}
  let mut db=self.db.lock().map_err(lock_err)?;let tx=db.transaction_with_behavior(TransactionBehavior::Immediate)?;
  known_layer(&tx,source)?;known_layer(&tx,target)?;if tx.query_row("SELECT count(*) FROM relation",[],|r|r.get::<_,u64>(0))?>=1024{return Err(Error::Limit)}let n=next(&tx,expected)?;
  tx.execute("INSERT INTO relation VALUES(?1,?2,?3,?4)",params![id.to_string(),source.to_string(),target.to_string(),kind])?;tx.commit()?;Ok(n)
 }
 pub fn apply(&self,b:&Batch)->Result<Receipt>{
  if b.operation.is_nil()||b.edits.len()+b.links.len()==0||b.edits.len()+b.links.len()>MAX_BATCH{return Err(Error::Limit)}
  let encoded=serde_json::to_vec(b).map_err(|_|Error::Invalid)?;
  if encoded.len()>MAX_BYTES{return Err(Error::Limit)}
  let digest=Sha256::digest(&encoded);
  let mut db=self.db.lock().map_err(lock_err)?;let tx=db.transaction_with_behavior(TransactionBehavior::Immediate)?;
  let old=tx.query_row("SELECT digest,reply FROM receipt WHERE operation=?1",[b.operation.to_string()],|r|Ok((r.get::<_,Vec<u8>>(0)?,r.get::<_,String>(1)?))).optional()?;
  if let Some((hash,reply))=old{if hash.as_slice()!=&digest[..]{return Err(Error::Reused)}return decode(&reply)}
  if tx.query_row("SELECT EXISTS(SELECT 1 FROM legacy_receipt WHERE operation=?1 UNION ALL SELECT 1 FROM legacy_command WHERE command=?1)",[b.operation.to_string()],|r|r.get::<_,bool>(0))?{return Err(Error::Reused)}
  if tx.query_row("SELECT EXISTS(SELECT 1 FROM bulk_import WHERE operation=?1)",[b.operation.to_string()],|r|r.get::<_,bool>(0))?{return Err(Error::Reused)}
  let n=next(&tx,b.expected_project)?;
  let mut seen=HashSet::new();let mut before=Vec::new();let mut features=Vec::new();
  let mut bytes=0usize;
  for e in &b.edits{
   known_layer(&tx,e.layer)?;
   if e.feature.is_nil()||!seen.insert((e.layer,e.feature)){return Err(Error::Invalid)}
   let (r,img)=current(&tx,e.layer,e.feature)?;
   if r!=e.expected||e.image.is_none()&&img.is_none(){return Err(Error::Conflict)}
   bytes=bytes.saturating_add(img.as_ref().map_or(0,|x|x.wkb.len()+x.attributes.to_string().len()));
   if bytes>MAX_BYTES{return Err(Error::Limit)}
   before.push(Edit{layer:e.layer,feature:e.feature,expected:r,image:img});
   let r=r.checked_add(1).filter(|r|*r<=i64::MAX as u64).ok_or(Error::Limit)?;
   tx.execute("INSERT INTO feature_revision VALUES(?1,?2,?3) ON CONFLICT(layer,id) DO UPDATE SET revision=excluded.revision",params![e.layer.to_string(),e.feature.to_string(),r])?;
   if let Some(img)=&e.image{
    let (bb,a)=validate_image(&tx,e.layer,img)?;
    tx.execute("INSERT INTO feature(layer,id,wkb,attributes,minx,maxx,miny,maxy) VALUES(?1,?2,?3,?4,?5,?6,?7,?8) ON CONFLICT(layer,id) DO UPDATE SET wkb=excluded.wkb,attributes=excluded.attributes,minx=excluded.minx,maxx=excluded.maxx,miny=excluded.miny,maxy=excluded.maxy",params![e.layer.to_string(),e.feature.to_string(),&img.wkb,a,bb[0],bb[1],bb[2],bb[3]])?;
   }else{tx.execute("DELETE FROM feature WHERE layer=?1 AND id=?2",params![e.layer.to_string(),e.feature.to_string()])?;}
   features.push((e.layer,e.feature,r));
  }
  let mut link_before=Vec::new();let mut link_seen=HashSet::new();let mut rels=HashSet::new();
  for l in &b.links{
   if !link_seen.insert((l.relation,l.source,l.target)){return Err(Error::Invalid)}
   let (s,t):(String,String)=tx.query_row("SELECT source_layer,target_layer FROM relation WHERE id=?1",[l.relation.to_string()],|r|Ok((r.get(0)?,r.get(1)?)))?;
   let present=tx.query_row("SELECT EXISTS(SELECT 1 FROM edge WHERE relation=?1 AND source_id=?2 AND target_id=?3)",params![l.relation.to_string(),l.source.to_string(),l.target.to_string()],|r|r.get::<_,bool>(0))?;
   if present==l.add{return Err(Error::Conflict)}
   link_before.push(Link{relation:l.relation,source:l.source,target:l.target,add:present});
   if l.add{tx.execute("INSERT INTO edge VALUES(?1,?2,?3,?4,?5)",params![l.relation.to_string(),s,l.source.to_string(),t,l.target.to_string()])?;}
   else{tx.execute("DELETE FROM edge WHERE relation=?1 AND source_id=?2 AND target_id=?3",params![l.relation.to_string(),l.source.to_string(),l.target.to_string()])?;}
   rels.insert(l.relation);
  }
  for id in rels{
   let k:String=tx.query_row("SELECT kind FROM relation WHERE id=?1",[id.to_string()],|r|r.get(0))?;
   if k!="many_many"&&tx.query_row("SELECT EXISTS(SELECT 1 FROM edge WHERE relation=?1 GROUP BY target_id HAVING count(*)>1)",[id.to_string()],|r|r.get::<_,bool>(0))?{return Err(Error::Relation)}
   if k=="one_one"&&tx.query_row("SELECT EXISTS(SELECT 1 FROM edge WHERE relation=?1 GROUP BY source_id HAVING count(*)>1)",[id.to_string()],|r|r.get::<_,bool>(0))?{return Err(Error::Relation)}
  }
  let receipt=Receipt{revision:n,features};
  tx.execute("INSERT INTO receipt VALUES(?1,?2,?3,?4)",params![b.operation.to_string(),&digest[..],n,json(&receipt)?])?;
  tx.execute("INSERT INTO journal VALUES(?1,?2,?3,?4)",params![n,b.operation.to_string(),json(&(before,link_before))?,json(b)?])?;
  tx.commit().map_err(|e|match e{rusqlite::Error::SqliteFailure(ref info,_) if info.code==rusqlite::ErrorCode::ConstraintViolation=>Error::Relation,_=>Error::Storage})?;Ok(receipt)
 }
 // Undo the last feature/link batch via a new transaction. Undoing the
 // compensating batch redoes it; never rewind feature or project revisions.
 // Metadata changes do not have a compensation record.
 pub fn compensate_latest(&self,operation:Uuid,expected_project:u64)->Result<Receipt>{
  let (mut edits,links):(Vec<Edit>,Vec<Link>)={
    let db=self.db.lock().map_err(lock_err)?;
    let data:String=db.query_row("SELECT before_json FROM journal WHERE revision=?1",[expected_project],|r|r.get(0)).optional()?.ok_or(Error::Unsupported)?;
    decode(&data)?
  };
  for e in &mut edits{e.expected=e.expected.checked_add(1).ok_or(Error::Limit)?;}
  self.apply(&Batch{operation,expected_project,edits,links})
 }
 pub fn query(&self,layer:Uuid,b:[f64;4],after:i64,limit:u32,expected:u64)->Result<Page>{
  if b.iter().any(|v|!v.is_finite())||b[0]>b[2]||b[1]>b[3]||after<0||!(1..=200).contains(&limit){return Err(Error::Invalid)}
  let mut db=self.db.lock().map_err(lock_err)?;let tx=db.transaction()?;
  if rev(&tx)?!=expected{return Err(Error::Conflict)}known_layer(&tx,layer)?;
  let mut q=tx.prepare("SELECT f.fid,f.id,r.revision,f.wkb,f.attributes FROM feature_index i JOIN feature f ON f.fid=i.fid JOIN feature_revision r ON r.layer=f.layer AND r.id=f.id WHERE f.layer=?1 AND f.fid>?2 AND i.maxx>=?3 AND i.maxy>=?4 AND i.minx<=?5 AND i.miny<=?6 AND f.maxx>=?3 AND f.maxy>=?4 AND f.minx<=?5 AND f.miny<=?6 ORDER BY f.fid LIMIT ?7")?;
  let mut rows=q.query(params![layer.to_string(),after,b[0],b[1],b[2],b[3],limit+1])?;
  let mut out=Vec::new();let mut size=0usize;let mut more=false;
  while let Some(r)=rows.next()?{
   if out.len()==limit as usize{more=true;break}
   let w:Vec<u8>=r.get(3)?;let a:String=r.get(4)?;
   if size+w.len()+a.len()>MAX_BYTES{if out.is_empty(){return Err(Error::Limit)}more=true;break}
   size+=w.len()+a.len();
   out.push(Row{fid:r.get(0)?,layer,id:Uuid::parse_str(&r.get::<_,String>(1)?).map_err(|_|Error::Invalid)?,revision:r.get(2)?,image:Image{wkb:w,attributes:decode(&a)?}});
  }
  let next=if more{out.last().map(|r|r.fid)}else{None};
  Ok(Page{revision:expected,rows:out,next})
 }
 // Typed virtual join: one relation, one source, bounded target IDs; no SQL from renderer.
 pub fn related(&self,relation:Uuid,source:Uuid,after:&str,limit:u32,expected:u64)->Result<Vec<Uuid>>{
  if after.len()>36||!(1..=200).contains(&limit){return Err(Error::Invalid)}
  let db=self.db.lock().map_err(lock_err)?;if rev(&db)?!=expected{return Err(Error::Conflict)}
  let mut q=db.prepare("SELECT target_id FROM edge WHERE relation=?1 AND source_id=?2 AND target_id>?3 ORDER BY target_id LIMIT ?4")?;
  let rows=q.query_map(params![relation.to_string(),source.to_string(),after,limit],|r|r.get::<_,String>(0))?;
  rows.map(|r|Uuid::parse_str(&r?).map_err(|_|Error::Invalid)).collect()
 }
 pub fn integrity(&self)->Result<bool>{
  let db=self.db.lock().map_err(lock_err)?;
  let v:String=db.pragma_query_value(None,"quick_check",|r|r.get(0))?;
  let fk:i64=db.query_row("SELECT count(*) FROM pragma_foreign_key_check",[],|r|r.get(0))?;
  let bad:i64=db.query_row("SELECT count(*) FROM feature f LEFT JOIN feature_index i ON i.fid=f.fid WHERE i.fid IS NULL OR i.minx>f.minx OR i.maxx<f.maxx OR i.miny>f.miny OR i.maxy<f.maxy",[],|r|r.get(0))?;
  let orphan:i64=db.query_row("SELECT count(*) FROM feature_index i LEFT JOIN feature f ON f.fid=i.fid WHERE f.fid IS NULL",[],|r|r.get(0))?;
  Ok(v=="ok"&&fk==0&&bad==0&&orphan==0&&asset_store::verify_assets(&db)?&&survey::verify(&db)?)
 }
}

#[cfg(test)]
mod tests{
 use super::*;
 fn img(x:f64)->Image{Image{wkb:ovg_mobile_engine::edit::draft("point",&[x,11.]).unwrap(),attributes:serde_json::json!({"name":"A"})}}
 fn e(l:Uuid,id:Uuid,expected:u64,image:Option<Image>)->Edit{Edit{layer:l,feature:id,expected,image}}
 #[test]fn multi_layer_atomic_replay_and_restart(){
   let d=tempfile::tempdir().unwrap();let p=d.path().join("project.sqlite");
   let s=Store::create(&p).unwrap();let a=Uuid::new_v4();let b=Uuid::new_v4();let x=Uuid::new_v4();let y=Uuid::new_v4();
   s.add_layer(a,"A",0).unwrap();s.add_layer(b,"B",1).unwrap();
   let bad=Batch{operation:Uuid::new_v4(),expected_project:2,edits:vec![e(a,x,0,Some(img(106.))),e(b,y,7,Some(img(107.)))],links:vec![]};
   assert_eq!(s.apply(&bad),Err(Error::Conflict));assert_eq!(s.revision().unwrap(),2);
   assert!(s.query(a,[0.,0.,180.,90.],0,10,2).unwrap().rows.is_empty());
   let good=Batch{operation:Uuid::new_v4(),expected_project:2,edits:vec![e(a,x,0,Some(img(106.))),e(b,y,0,Some(img(107.)))],links:vec![]};
   let r=s.apply(&good).unwrap();assert_eq!(r.revision,3);assert!(s.integrity().unwrap());drop(s);
   let s=Store::reopen(&p).unwrap();assert_eq!(s.apply(&good).unwrap(),r);
   let mut changed=good.clone();changed.edits[0].image=Some(img(105.));assert_eq!(s.apply(&changed),Err(Error::Reused));
   assert_eq!(s.query(b,[106.,10.,108.,12.],0,10,3).unwrap().rows[0].id,y);
   assert!(s.query(a,[0.,0.,180.,90.],0,10,2).is_err());
   let undo=Uuid::new_v4();let u=s.compensate_latest(undo,3).unwrap();assert_eq!(u.revision,4);
   assert_eq!(s.compensate_latest(undo,3).unwrap(),u);
   assert!(s.query(a,[0.,0.,180.,90.],0,10,4).unwrap().rows.is_empty());
   assert_eq!(s.compensate_latest(Uuid::new_v4(),4).unwrap().revision,5);
   assert_eq!(s.query(a,[0.,0.,180.,90.],0,10,5).unwrap().rows[0].revision,3);
 }
 #[test]fn relation_cardinality_and_deletion_roll_back(){
   let d=tempfile::tempdir().unwrap();let s=Store::create(&d.path().join("p.sqlite")).unwrap();
   let a=Uuid::new_v4();let b=Uuid::new_v4();let rel=Uuid::new_v4();let x=Uuid::new_v4();let y=Uuid::new_v4();let z=Uuid::new_v4();
   s.add_layer(a,"A",0).unwrap();s.add_layer(b,"B",1).unwrap();s.add_relation(rel,a,b,"one_many",2).unwrap();
   let l=Link{relation:rel,source:x,target:y,add:true};
   let batch=Batch{operation:Uuid::new_v4(),expected_project:3,edits:vec![e(a,x,0,Some(img(106.))),e(a,z,0,Some(img(106.))),e(b,y,0,Some(img(107.)))],links:vec![l.clone()]};
   s.apply(&batch).unwrap();
   let invalid=Batch{operation:Uuid::new_v4(),expected_project:4,edits:vec![],links:vec![Link{source:z,..l.clone()}]};
   assert_eq!(s.apply(&invalid),Err(Error::Relation));
   let del=Batch{operation:Uuid::new_v4(),expected_project:4,edits:vec![e(b,y,1,None)],links:vec![]};
   assert_eq!(s.apply(&del),Err(Error::Relation));
   assert_eq!(s.related(rel,x,"",10,4).unwrap(),vec![y]);
   let good=Batch{links:vec![Link{add:false,..l}],..del};s.apply(&good).unwrap();assert!(s.integrity().unwrap());
 }
 #[test]fn schema_validates_existing_and_new_rows(){
   let d=tempfile::tempdir().unwrap();let s=Store::create(&d.path().join("p.sqlite")).unwrap();let a=Uuid::new_v4();
   s.add_layer(a,"A",0).unwrap();
   let field=Field{name:"name".into(),kind:attributes::Kind::Text,required:true,max_length:Some(2),min:None,max:None};
   s.set_schema(a,&[field],1).unwrap();
   let mut bad=img(106.);bad.attributes=serde_json::json!({"name":"long"});
   let batch=Batch{operation:Uuid::new_v4(),expected_project:2,edits:vec![e(a,Uuid::new_v4(),0,Some(bad))],links:vec![]};
   assert_eq!(s.apply(&batch),Err(Error::Invalid));assert_eq!(s.revision().unwrap(),2);
 }
}
