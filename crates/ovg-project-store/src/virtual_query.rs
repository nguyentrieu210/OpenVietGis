//! Bounded typed query plans; no SQL or dynamic identifiers supplied by UI.
use super::*;
#[derive(Clone,Debug,Serialize,Deserialize)]
#[serde(tag="op",rename_all="snake_case",deny_unknown_fields)]
pub enum Filter{Exists{field:String},Eq{field:String,value:Value},Contains{field:String,text:String},NumberRange{field:String,min:f64,max:f64}}
impl Filter{
 fn valid(&self)->bool{
  let f=match self{Self::Exists{field}|Self::Eq{field,..}|Self::Contains{field,..}|Self::NumberRange{field,..}=>field};
  if f.is_empty()||f.len()>128||f.chars().any(char::is_control){return false}
  match self{Self::Eq{value,..}=>!value.is_array()&&!value.is_object()&&value.to_string().len()<=4096,Self::Contains{text,..}=>text.len()<=256,Self::NumberRange{min,max,..}=>min.is_finite()&&max.is_finite()&&min<=max,_=>true}
 }
 fn matches(&self,a:&Value)->bool{match self{
  Self::Exists{field}=>a.get(field).is_some_and(|v|!v.is_null()),
  Self::Eq{field,value}=>a.get(field)==Some(value),
  Self::Contains{field,text}=>a.get(field).and_then(Value::as_str).is_some_and(|v|v.contains(text)),
  Self::NumberRange{field,min,max}=>a.get(field).and_then(Value::as_f64).is_some_and(|n|n>=*min&&n<=*max)
 }}
}
#[derive(Clone,Debug,Serialize,Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Plan{pub layer:Uuid,pub bbox:[f64;4],pub filters:Vec<Filter>,pub relation:Option<RelationFilter>}
#[derive(Clone,Debug,Serialize,Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RelationFilter{pub relation:Uuid,pub source:Uuid}
#[derive(Clone,Debug,Serialize,Deserialize)]
pub struct Cursor{pub revision:u64,pub plan_digest:String,pub after:i64}
#[derive(Debug,Serialize)]
pub struct VirtualPage{pub rows:Vec<Row>,pub next:Option<Cursor>,pub scanned:u32}
impl Store{
 pub fn virtual_page(&self,p:&Plan,cursor:Option<&Cursor>,limit:u32,expected:u64)->Result<VirtualPage>{
  if p.filters.len()>16||!p.filters.iter().all(Filter::valid)||!(1..=200).contains(&limit){return Err(Error::Invalid)}
  let digest=format!("{:x}",Sha256::digest(serde_json::to_vec(p).map_err(|_|Error::Invalid)?));
  let after=match cursor{None=>0,Some(c) if c.revision==expected&&c.plan_digest==digest&&c.after>=0=>c.after,_=>return Err(Error::Conflict)};
  if let Some(r)=&p.relation{
   let db=self.db.lock().map_err(lock_err)?;
   if rev(&db)?!=expected{return Err(Error::Conflict)}
   let t:String=db.query_row("SELECT target_layer FROM relation WHERE id=?1",[r.relation.to_string()],|r|r.get(0))?;
   if t!=p.layer.to_string(){return Err(Error::Invalid)}
  }
  // One bounded source page per call, including empty filtered pages with a continuation.
  let base=self.query(p.layer,p.bbox,after,200,expected)?;
  let db=self.db.lock().map_err(lock_err)?;
  if rev(&db)?!=expected{return Err(Error::Conflict)}
  let mut result=Vec::new();let mut last=after;let mut scanned=0;
  let mut more=base.next.is_some();
  for row in base.rows{
   if result.len()==limit as usize{more=true;break}
   last=row.fid;scanned+=1;
   if !p.filters.iter().all(|f|f.matches(&row.image.attributes)){continue}
   if let Some(r)=&p.relation{
    let found:bool=db.query_row("SELECT EXISTS(SELECT 1 FROM edge WHERE relation=?1 AND source_id=?2 AND target_id=?3)",params![r.relation.to_string(),r.source.to_string(),row.id.to_string()],|r|r.get(0))?;
    if !found{continue}
   }
   result.push(row);
  }
  Ok(VirtualPage{rows:result,next:if more{Some(Cursor{revision:expected,plan_digest:digest,after:last})}else{None},scanned})
 }
}
