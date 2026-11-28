//! Foreground track journal. Durable segments and explicit resume, never silently bridge a gap.
use super::{LocalStore,Error};
use ovg_mobile_engine::survey::{Fix,validate_fix};
use rusqlite::{params,OptionalExtension,TransactionBehavior};
use uuid::Uuid;
impl LocalStore{
 pub fn create_track(&self,name:&str)->Result<Uuid,Error>{
  if name.trim().is_empty()||name.len()>128||name.chars().any(char::is_control){return Err(Error::Invalid);}
  let id=Uuid::new_v4();let db=self.lock()?;
  db.execute("INSERT INTO ovg_survey_track(id,name,state) VALUES(?1,?2,'paused')",params![id.to_string(),name])?;Ok(id)
 }
 pub fn track_state(&self,id:Uuid,action:&str)->Result<(),Error>{
  let db=self.lock()?;
  let n=match action{
   "resume"=>db.execute("UPDATE ovg_survey_track SET state='recording',segment=segment+1 WHERE id=?1 AND state='paused'",[id.to_string()])?,
   "pause"=>db.execute("UPDATE ovg_survey_track SET state='paused' WHERE id=?1 AND state='recording'",[id.to_string()])?,
   "finish"=>db.execute("UPDATE ovg_survey_track SET state='finished' WHERE id=?1 AND state IN('recording','paused')",[id.to_string()])?,
   _=>return Err(Error::Invalid)
  };if n!=1{return Err(Error::Conflict);}Ok(())
 }
 /// Called by the owning app after process restart before offering explicit resume.
 pub fn recover_tracks(&self)->Result<usize,Error>{
  let db=self.lock()?;Ok(db.execute("UPDATE ovg_survey_track SET state='paused' WHERE state='recording'",[])?)
 }
 pub fn append_fix(&self,id:Uuid,fix:&Fix)->Result<u64,Error>{
  validate_fix(fix).map_err(|_|Error::Invalid)?;
  let mut db=self.lock()?;let tx=db.transaction_with_behavior(TransactionBehavior::Immediate)?;
  let segment:u64=tx.query_row("SELECT segment FROM ovg_survey_track WHERE id=?1 AND state='recording'",[id.to_string()],|r|r.get(0)).optional()?.ok_or(Error::Conflict)?;
  let prev:Option<(u64,i64)>=tx.query_row("SELECT seq,utc_ms FROM ovg_survey_fix WHERE track_id=?1 ORDER BY seq DESC LIMIT 1",[id.to_string()],|r|Ok((r.get(0)?,r.get(1)?))).optional()?;
  if prev.is_some_and(|(seq,t)|seq>=100000||fix.utc_ms<=t){return Err(Error::Conflict);}
  let seq=prev.map_or(1,|(s,_)|s+1);
  tx.execute("INSERT INTO ovg_survey_fix VALUES(?1,?2,?3,?4,?5)",params![id.to_string(),seq,segment,fix.utc_ms,serde_json::to_string(fix).map_err(|_|Error::Invalid)?])?;
  tx.commit()?;Ok(seq)
 }
 /// GPX 1.1 track export; time/quality and accuracy loss explicitly reported.
 /// Not yet an independently qualified GPX export path.
 pub fn track_gpx(&self,id:Uuid)->Result<String,Error>{
  let db=self.lock()?;let name:String=db.query_row("SELECT name FROM ovg_survey_track WHERE id=?1",[id.to_string()],|r|r.get(0)).optional()?.ok_or(Error::Invalid)?;
  fn xml(s:&str)->String{s.replace('&',"&amp;").replace('<',"&lt;").replace('>',"&gt;").replace('"',"&quot;").replace('\'',"&apos;")}
  let mut out=format!("<?xml version=\"1.0\" encoding=\"UTF-8\"?><gpx version=\"1.1\" creator=\"OpenVietGIS\" xmlns=\"http://www.topografix.com/GPX/1/1\"><trk><name>{}</name>",xml(&name));
  let mut stmt=db.prepare("SELECT segment,fix_json FROM ovg_survey_fix WHERE track_id=?1 ORDER BY seq")?;
  let mut rows=stmt.query([id.to_string()])?;let mut segment=None;
  while let Some(r)=rows.next()?{
   let s:u64=r.get(0)?;let f:Fix=serde_json::from_str(&r.get::<_,String>(1)?).map_err(|_|Error::Invalid)?;validate_fix(&f).map_err(|_|Error::Invalid)?;
   if segment!=Some(s){if segment.is_some(){out.push_str("</trkseg>");}out.push_str("<trkseg>");segment=Some(s);}
   out.push_str(&format!("<trkpt lat=\"{}\" lon=\"{}\">",f.lat,f.lon));
   if let Some(z)=f.altitude_m{out.push_str(&format!("<ele>{z}</ele>"));}
   let ts:String=db.query_row("SELECT strftime('%Y-%m-%dT%H:%M:%fZ',?1/1000.0,'unixepoch')",[f.utc_ms],|r|r.get(0))?;
   out.push_str(&format!("<time>{ts}</time></trkpt>"));
   if out.len()>16*1024*1024{return Err(Error::Limit);}
  }
  if segment.is_some(){out.push_str("</trkseg>");}out.push_str("</trk></gpx>");Ok(out)
 }
}
