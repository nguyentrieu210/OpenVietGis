//! Canonical survey data, shared by desktop and mobile, with original fix quality
//! and track segment boundaries. A migrated recording is paused, never resumed implicitly.
use super::*;
use ovg_mobile_engine::survey::{Fix,validate_fix};
use std::sync::atomic::{AtomicBool,Ordering};
#[derive(Clone,Debug,Serialize)]
pub struct Track{pub layer:Uuid,pub id:Uuid,pub name:String,pub state:String,pub segment:u64}
#[derive(Clone,Debug,Serialize)]
pub struct Sample{pub seq:u64,pub segment:u64,pub fix:Fix}
#[derive(Clone,Debug,Serialize)]
pub struct SurveyPage{pub revision:String,pub rows:Vec<Sample>,pub next:Option<String>}
fn uuid(s:&str)->Result<Uuid>{let u=Uuid::parse_str(s).map_err(|_|Error::Invalid)?;if u.is_nil()||u.to_string()!=s{return Err(Error::Invalid)}Ok(u)}
fn name_ok(name:&str)->bool{!name.trim().is_empty()&&name.len()<=128&&!name.chars().any(char::is_control)}
fn validate_sample(seq:u64,seg:u64,utc:i64,raw:&str,prev:(u64,u64,i64),max:u64)->Result<Fix>{
 if raw.len()>4096||seq!=prev.0+1||seq>100_000||seg==0||seg<prev.1||seg>max||utc<=prev.2{return Err(Error::Invalid)}
 let f:Fix=decode(raw)?;validate_fix(&f).map_err(|_|Error::Invalid)?;if f.utc_ms!=utc{return Err(Error::Invalid)}Ok(f)
}
pub(crate) fn migrate_layer(tx:&rusqlite::Transaction<'_>,layer:Uuid,legacy:&Connection,cancel:&AtomicBool)->Result<(u64,u64)>{
 let l=layer.to_string();let mut tracks=0;let mut fixes=0;
 let mut q=legacy.prepare("SELECT id,name,state,segment FROM ovg_survey_track ORDER BY id")?;
 let mut rows=q.query([])?;
 while let Some(r)=rows.next()?{
  if cancel.load(Ordering::Acquire){return Err(Error::Conflict)}
  let id:String=r.get(0)?;uuid(&id)?;let name:String=r.get(1)?;let state:String=r.get(2)?;let segment:u64=r.get(3)?;
  if !name_ok(&name)||!matches!(state.as_str(),"recording"|"paused"|"finished")||segment>i64::MAX as u64{return Err(Error::Invalid)}
  tracks+=1;if tracks>10_000{return Err(Error::Limit)}
  tx.execute("INSERT INTO survey_track VALUES(?1,?2,?3,?4,?5)",params![l,id,name,if state=="recording"{"paused"}else{&state},segment])?;
  let mut q=legacy.prepare("SELECT seq,segment,utc_ms,fix_json FROM ovg_survey_fix WHERE track_id=?1 ORDER BY seq")?;
  let mut rs=q.query([&id])?;let mut prev=(0,0,0);
  while let Some(f)=rs.next()?{
   if cancel.load(Ordering::Acquire){return Err(Error::Conflict)}
   let seq:u64=f.get(0)?;let seg:u64=f.get(1)?;let utc:i64=f.get(2)?;let raw:String=f.get(3)?;
   validate_sample(seq,seg,utc,&raw,prev,segment)?;prev=(seq,seg,utc);
   fixes+=1;if fixes>1_000_000{return Err(Error::Limit)}
   tx.execute("INSERT INTO survey_fix VALUES(?1,?2,?3,?4,?5,?6)",params![l,id,seq,seg,utc,raw])?;
  }
 }
 let n:u64=legacy.query_row("SELECT count(*) FROM ovg_survey_fix",[],|r|r.get(0))?;
 if n!=fixes{return Err(Error::Invalid)}Ok((tracks,fixes))
}
pub(crate) fn verify(db:&Connection)->Result<bool>{
 let mut q=db.prepare("SELECT layer,id,name,state,segment FROM survey_track ORDER BY layer,id")?;
 let mut rows=q.query([])?;let mut total=0u64;let mut tracks=0u64;
 while let Some(r)=rows.next()?{
  let l:String=r.get(0)?;let id:String=r.get(1)?;let name:String=r.get(2)?;let state:String=r.get(3)?;let max:u64=r.get(4)?;
  tracks+=1;if tracks>330_000||uuid(&l).is_err()||uuid(&id).is_err()||!name_ok(&name)||!matches!(state.as_str(),"recording"|"paused"|"finished"){return Ok(false)}
  let mut q=db.prepare("SELECT seq,segment,utc_ms,fix_json FROM survey_fix WHERE layer=?1 AND track_id=?2 ORDER BY seq")?;
  let mut rs=q.query(params![l,id])?;let mut prev=(0,0,0);
  while let Some(f)=rs.next()?{
   let seq:u64=f.get(0)?;let seg:u64=f.get(1)?;let utc:i64=f.get(2)?;let raw:String=f.get(3)?;
   if validate_sample(seq,seg,utc,&raw,prev,max).is_err(){return Ok(false)}prev=(seq,seg,utc);total+=1;
  }
 }
 let n:u64=db.query_row("SELECT count(*) FROM survey_fix",[],|r|r.get(0))?;Ok(n==total)
}
impl Store{
 pub fn survey_tracks(&self,layer:Uuid,after:&str,limit:u32,expected:u64)->Result<Vec<Track>>{
  if !(1..=200).contains(&limit)||!after.is_empty()&&uuid(after).is_err(){return Err(Error::Invalid)}
  let db=self.db.lock().map_err(lock_err)?;if rev(&db)?!=expected{return Err(Error::Conflict)}known_layer(&db,layer)?;
  let mut q=db.prepare("SELECT id,name,state,segment FROM survey_track WHERE layer=?1 AND id>?2 ORDER BY id LIMIT ?3")?;
  let mut rows=q.query(params![layer.to_string(),after,limit])?;let mut out=Vec::new();
  while let Some(r)=rows.next()?{out.push(Track{layer,id:uuid(&r.get::<_,String>(0)?)?,name:r.get(1)?,state:r.get(2)?,segment:r.get(3)?});}Ok(out)
 }
 pub fn survey_fixes(&self,layer:Uuid,track:Uuid,after:u64,limit:u32,expected:u64)->Result<SurveyPage>{
  if !(1..=200).contains(&limit)||after>100_000{return Err(Error::Invalid)}
  let db=self.db.lock().map_err(lock_err)?;if rev(&db)?!=expected{return Err(Error::Conflict)}known_layer(&db,layer)?;
  let l=layer.to_string();let t=track.to_string();
  if !db.query_row("SELECT EXISTS(SELECT 1 FROM survey_track WHERE layer=?1 AND id=?2)",params![l,t],|r|r.get::<_,bool>(0))?{return Err(Error::Invalid)}
  let mut q=db.prepare("SELECT seq,segment,fix_json FROM survey_fix WHERE layer=?1 AND track_id=?2 AND seq>?3 ORDER BY seq LIMIT ?4")?;
  let mut rows=q.query(params![l,t,after,limit+1])?;let mut out=Vec::new();let mut next=None;
  while let Some(r)=rows.next()?{
   if out.len()==limit as usize{next=out.last().map(|v:&Sample|v.seq.to_string());break}
   let raw:String=r.get(2)?;if raw.len()>4096{return Err(Error::Invalid)}
   let f:Fix=decode(&raw)?;validate_fix(&f).map_err(|_|Error::Invalid)?;
   out.push(Sample{seq:r.get(0)?,segment:r.get(1)?,fix:f});
  }Ok(SurveyPage{revision:expected.to_string(),rows:out,next})
 }
}
