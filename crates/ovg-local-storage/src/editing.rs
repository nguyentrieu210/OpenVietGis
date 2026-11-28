//! Atomic, persisted local edit commands. Undo/redo create new journal revisions.
use super::{LocalStore,Mutation,Error,unpack};
use ovg_mobile_engine::{edit,attributes};
use rusqlite::{params,OptionalExtension,Transaction,TransactionBehavior};
use serde::{Serialize,Deserialize};
use serde_json::{Value,json};
use uuid::Uuid;
#[derive(Clone,Debug,Serialize,Deserialize,PartialEq)]
struct Image{wkb:Vec<u8>,attributes:Value}
#[derive(Clone,Debug,Serialize,Deserialize,PartialEq)]
struct Change{id:Uuid,image:Option<Image>}
#[derive(Clone,Debug,Serialize,Deserialize)]
pub struct Edit{pub feature_id:Uuid,pub expected_revision:u64,pub wkb:Option<Vec<u8>>,pub attributes:Value}
fn read(tx:&Transaction<'_>,id:Uuid)->Result<(u64,Option<Image>),Error>{
 let rev=tx.query_row("SELECT revision FROM ovg_feature_revision WHERE stable_id=?1",[id.to_string()],|r|r.get::<_,u64>(0)).optional()?.unwrap_or(0);
 let row=tx.query_row("SELECT geom,attributes FROM features WHERE stable_id=?1",[id.to_string()],|r|Ok((r.get::<_,Vec<u8>>(0)?,r.get::<_,String>(1)?))).optional()?;
 let img=row.map(|(g,a)|->Result<Image,Error>{Ok(Image{wkb:unpack(&g)?.to_vec(),attributes:serde_json::from_str(&a).map_err(|_|Error::Invalid)?})}).transpose()?;
 Ok((rev,img))
}
fn schema(tx:&Transaction<'_>)->Result<Option<Vec<attributes::Field>>,Error>{
 let s:Option<String>=tx.query_row("SELECT fields_json FROM ovg_attribute_schema WHERE singleton=1",[],|r|r.get(0)).optional()?;
 s.map(|s|serde_json::from_str(&s).map_err(|_|Error::Invalid)).transpose()
}
impl LocalStore{
 pub(super) fn validate_attributes_in(tx:&Transaction<'_>,attrs:&Value)->Result<(),Error>{
    if let Some(fields)=schema(tx)?{attributes::validate(&fields,attrs).map_err(|_|Error::Invalid)?;}Ok(())
 }
 pub fn set_attribute_schema(&self,fields:&[attributes::Field])->Result<(),Error>{
    attributes::validate_schema(fields).map_err(|_|Error::Invalid)?;
    let mut db=self.lock()?;let tx=db.transaction_with_behavior(TransactionBehavior::Immediate)?;
    {let mut stmt=tx.prepare("SELECT attributes FROM features")?;let mut rows=stmt.query([])?;
     while let Some(r)=rows.next()?{let v:Value=serde_json::from_str(&r.get::<_,String>(0)?).map_err(|_|Error::Invalid)?;attributes::validate(fields,&v).map_err(|_|Error::Conflict)?;}}
    let text=serde_json::to_string(fields).map_err(|_|Error::Invalid)?;
    tx.execute("INSERT INTO ovg_attribute_schema VALUES(1,?1) ON CONFLICT(singleton) DO UPDATE SET fields_json=excluded.fields_json",[text])?;
    tx.execute("DELETE FROM ovg_edit_history",[])?;tx.commit()?;Ok(())
 }
 pub fn attribute_page(&self,after:i64,limit:u32,contains:&str)->Result<String,Error>{
    if after<0||limit==0||limit>200||contains.len()>128{return Err(Error::Invalid);}
    let db=self.lock()?;let mut stmt=db.prepare("SELECT fid,stable_id,revision,attributes FROM features WHERE fid>?1 AND instr(lower(attributes),lower(?2))>0 ORDER BY fid LIMIT ?3")?;
    let mut rows=stmt.query(params![after,contains,limit])?;let mut out=Vec::new();let mut size=0usize;
    while let Some(r)=rows.next()?{
      let a:String=r.get(3)?;size+=a.len();if size>1024*1024{break;}
      out.push(json!({"fid":r.get::<_,i64>(0)?,"id":r.get::<_,String>(1)?,"revision":r.get::<_,u64>(2)?,"attributes":serde_json::from_str::<Value>(&a).map_err(|_|Error::Invalid)?}));
    }
    Ok(serde_json::to_string(&out).map_err(|_|Error::Invalid)?)
 }
 pub fn feature_json(&self,id:Uuid)->Result<String,Error>{
    let mut db=self.lock()?;let tx=db.transaction()?;
    let (revision,img)=read(&tx,id)?;let img=img.ok_or(Error::Conflict)?;
    let geometry=ovg_spatial::parse_xy_wkb(&img.wkb,&ovg_spatial::ValidationLimits::default()).map_err(|_|Error::Invalid)?;
    Ok(json!({"id":id,"revision":revision,"geometry":ovg_spatial::to_geojson(&geometry.geometry),"attributes":img.attributes}).to_string())
 }
 pub fn commit_edit(&self,command:Uuid,edits:&[Edit])->Result<(),Error>{
    if command.is_nil()||edits.is_empty()||edits.len()>32{return Err(Error::Invalid);}
    let mut db=self.lock()?;let tx=db.transaction_with_behavior(TransactionBehavior::Immediate)?;
    if tx.query_row("SELECT 1 FROM ovg_edit_history WHERE command_id=?1",[command.to_string()],|r|r.get::<_,i32>(0)).optional()?.is_some(){return Err(Error::Reused);}
    let mut ids=std::collections::HashSet::new();let mut before=Vec::new();let mut after=Vec::new();let mut bytes=0usize;
    for e in edits{
      if !ids.insert(e.feature_id)||e.feature_id.is_nil(){return Err(Error::Invalid);}
      let (rev,img)=read(&tx,e.feature_id)?;
      if rev!=e.expected_revision{return Err(Error::Conflict);}
      if let Some(w)=&e.wkb{edit::validate(w).map_err(|_|Error::Invalid)?;}
      bytes+=img.as_ref().map_or(0,|i|i.wkb.len()+i.attributes.to_string().len())+e.wkb.as_ref().map_or(0,Vec::len)+e.attributes.to_string().len();
      if bytes>2*1024*1024{return Err(Error::Limit);}
      before.push(Change{id:e.feature_id,image:img});
      let m=Mutation{operation_id:Uuid::new_v4(),feature_id:e.feature_id,expected_revision:rev,wkb:e.wkb.clone(),attributes:e.attributes.clone()};
      Self::apply_in(&tx,&m)?;
      after.push(Change{id:e.feature_id,image:read(&tx,e.feature_id)?.1});
    }
    tx.execute("DELETE FROM ovg_edit_history WHERE applied=0",[])?;
    tx.execute("INSERT INTO ovg_edit_history(command_id,before_json,after_json,applied) VALUES(?1,?2,?3,1)",params![command.to_string(),serde_json::to_string(&before).map_err(|_|Error::Invalid)?,serde_json::to_string(&after).map_err(|_|Error::Invalid)?])?;
    tx.execute("DELETE FROM ovg_edit_history WHERE seq NOT IN (SELECT seq FROM ovg_edit_history ORDER BY seq DESC LIMIT 64)",[])?;
    tx.commit()?;Ok(())
 }
 pub fn history_step(&self,redo:bool)->Result<bool,Error>{
    let mut db=self.lock()?;let tx=db.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let sql=if redo{"SELECT seq,before_json,after_json FROM ovg_edit_history WHERE applied=0 ORDER BY seq ASC LIMIT 1"}else{"SELECT seq,before_json,after_json FROM ovg_edit_history WHERE applied=1 ORDER BY seq DESC LIMIT 1"};
    let row=tx.query_row(sql,[],|r|Ok((r.get::<_,i64>(0)?,r.get::<_,String>(1)?,r.get::<_,String>(2)?))).optional()?;
    let Some((seq,b,a))=row else{return Ok(false)};
    let b:Vec<Change>=serde_json::from_str(&b).map_err(|_|Error::Invalid)?;let a:Vec<Change>=serde_json::from_str(&a).map_err(|_|Error::Invalid)?;
    let (from,to)=if redo{(b,a)}else{(a,b)};
    if from.len()!=to.len(){return Err(Error::Invalid);}
    for (f,t) in from.iter().zip(to.iter()){
      let (rev,current)=read(&tx,f.id)?;
      if f.id!=t.id||current!=f.image{return Err(Error::Conflict);}
      if current==t.image{continue;}
      if let Some(img)=&t.image{edit::validate(&img.wkb).map_err(|_|Error::Invalid)?;}
      Self::apply_in(&tx,&Mutation{operation_id:Uuid::new_v4(),feature_id:f.id,expected_revision:rev,wkb:t.image.as_ref().map(|i|i.wkb.clone()),attributes:t.image.as_ref().map(|i|i.attributes.clone()).unwrap_or_else(||json!({}))})?;
    }
    tx.execute("UPDATE ovg_edit_history SET applied=?1 WHERE seq=?2",params![redo,seq])?;
    tx.commit()?;Ok(true)
 }
 pub fn snap_local(&self,x:f64,y:f64,tolerance_m:f64)->Result<String,Error>{
    if !x.is_finite()||!y.is_finite()||x.abs()>180.||y.abs()>85.||!tolerance_m.is_finite()||tolerance_m<=0.||tolerance_m>100.{return Err(Error::Invalid);}
    let dx=tolerance_m/(110000.*y.to_radians().cos());let dy=tolerance_m/110000.;
    let bbox=[x-dx,y-dy,x+dx,y+dy];
    if bbox[0]< -180.||bbox[2]>180.{return Err(Error::Unsupported);}
    let mut db=self.lock()?;let tx=db.transaction()?;
    let rows=Self::query_in(&tx,bbox,0,257)?;let last=rows.last().map_or(0,|r|r.0);
    if rows.len()>256||!Self::query_in(&tx,bbox,last,1)?.is_empty(){return Err(Error::Limit);}
    let mut best:Option<(Uuid,u64,edit::Snap)>=None;
    for (_,f) in rows{
      if let Some(hit)=edit::snap(&f.wkb,x,y,tolerance_m).map_err(|_|Error::Invalid)?{
        if best.as_ref().is_none_or(|(_,_,b)|hit.distance_m<b.distance_m-1e-9||((hit.distance_m-b.distance_m).abs()<=1e-9&&hit.vertex&&!b.vertex)){
          best=Some((f.feature_id,f.revision,hit));
        }
      }
    }
    Ok(match best{Some((id,rev,s))=>json!({"feature_id":id,"revision":rev,"x":s.x,"y":s.y,"distance_m":s.distance_m,"vertex":s.vertex}),None=>Value::Null}.to_string())
 }
}
