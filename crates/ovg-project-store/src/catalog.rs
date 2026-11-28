//! Typed project catalog and geometry interchange for the unified workspace.
use super::*;
#[derive(Debug,Serialize)]
pub struct Layer{pub id:Uuid,pub name:String,pub fields:Option<Vec<Field>>}
#[derive(Debug,Serialize)]
pub struct Relation{pub id:Uuid,pub source:Uuid,pub target:Uuid,pub kind:String}
#[derive(Debug,Serialize)]
pub struct Catalog{pub revision:String,pub layers:Vec<Layer>,pub relations:Vec<Relation>}
fn id(s:&str)->Result<Uuid>{Uuid::parse_str(s).map_err(|_|Error::Invalid)}
impl Store{
 pub fn catalog(&self)->Result<Catalog>{
  let mut db=self.db.lock().map_err(lock_err)?;let tx=db.transaction()?;
  let revision=rev(&tx)?.to_string();
  let layers={
   let mut q=tx.prepare("SELECT id,name,schema_json FROM layer ORDER BY name,id LIMIT 257")?;
   let rows=q.query_map([],|r|Ok((r.get::<_,String>(0)?,r.get::<_,String>(1)?,r.get::<_,Option<String>>(2)?)))?;
   let mut v=Vec::new();for r in rows{let (i,name,s)=r?;v.push(Layer{id:id(&i)?,name,fields:s.map(|x|decode(&x)).transpose()?});}
   if v.len()>256{return Err(Error::Limit)}v
  };
  let relations={
   let mut q=tx.prepare("SELECT id,source_layer,target_layer,kind FROM relation ORDER BY id LIMIT 1025")?;
   let rows=q.query_map([],|r|Ok((r.get::<_,String>(0)?,r.get::<_,String>(1)?,r.get::<_,String>(2)?,r.get::<_,String>(3)?)))?;
   let mut v=Vec::new();for r in rows{let (i,s,t,kind)=r?;v.push(Relation{id:id(&i)?,source:id(&s)?,target:id(&t)?,kind});}
   if v.len()>1024{return Err(Error::Limit)}v
  };Ok(Catalog{revision,layers,relations})
 }
 pub fn feature(&self,layer:Uuid,feature:Uuid,expected:u64)->Result<(u64,Image)>{
  let db=self.db.lock().map_err(lock_err)?;
  if rev(&db)?!=expected{return Err(Error::Conflict)}known_layer(&db,layer)?;
  let (r,i)=current(&db,layer,feature)?;Ok((r,i.ok_or(Error::Invalid)?))
 }
}
