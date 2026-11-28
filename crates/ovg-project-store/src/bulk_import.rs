//! Streaming import into a new layer. A single transaction, bounded record memory.
//! Bulk layer creation is not part of the interactive undo stack.
use super::*;
use std::sync::atomic::{AtomicBool,Ordering};
#[derive(Debug,Clone,Serialize,Deserialize,PartialEq,Eq)]
pub struct BulkReceipt{pub layer:Uuid,pub revision:u64,pub count:u64,pub digest:String}
#[derive(Serialize)]
pub struct BulkRow{pub id:Uuid,pub image:Image}
impl Store{
 pub fn import_layer_stream<I>(&self,operation:Uuid,layer:Uuid,name:&str,expected:u64,rows:I,cancel:&AtomicBool)->Result<BulkReceipt>
 where I:IntoIterator<Item=Result<BulkRow>>{
  if operation.is_nil()||layer.is_nil()||name.trim().is_empty()||name.len()>256{return Err(Error::Invalid)}
  let mut db=self.db.lock().map_err(lock_err)?;let tx=db.transaction_with_behavior(TransactionBehavior::Immediate)?;
  let op=operation.to_string();let key=layer.to_string();
  let existing:Option<(String,String,u64,u64)>=tx.query_row("SELECT digest,layer,revision,count FROM bulk_import WHERE operation=?1",[&op],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?))).optional()?;
  let revision=if let Some((_,old_layer,rev,_))=&existing{if old_layer!=&key{return Err(Error::Reused)}*rev}else{
   if tx.query_row("SELECT EXISTS(SELECT 1 FROM receipt WHERE operation=?1 UNION ALL SELECT 1 FROM legacy_receipt WHERE operation=?1 UNION ALL SELECT 1 FROM legacy_command WHERE command=?1)",[&op],|r|r.get::<_,bool>(0))?{return Err(Error::Reused)}
   if tx.query_row("SELECT count(*) FROM layer",[],|r|r.get::<_,u64>(0))?>=256{return Err(Error::Limit)}
   let n=next(&tx,expected)?;tx.execute("INSERT INTO layer VALUES(?1,?2,NULL)",params![key,name])?;n
  };
  let mut hash=Sha256::new();hash.update(layer.as_bytes());hash.update(expected.to_le_bytes());hash.update((name.len()as u64).to_le_bytes());hash.update(name.as_bytes());
  let mut count=0u64;let mut total=0usize;
  for row in rows{
   if cancel.load(Ordering::Acquire){return Err(Error::Conflict)}
   let row=row?;if row.id.is_nil(){return Err(Error::Invalid)}
   let encoded=serde_json::to_vec(&row).map_err(|_|Error::Invalid)?;
   total=total.saturating_add(encoded.len());if count>=100_000||total>256*1024*1024||encoded.len()>4*1024*1024{return Err(Error::Limit)}
   hash.update((encoded.len()as u64).to_le_bytes());hash.update(&encoded);
   if existing.is_none(){
    let (b,a)=validate_image(&tx,layer,&row.image)?;
    let id=row.id.to_string();
    tx.execute("INSERT INTO feature_revision VALUES(?1,?2,1)",params![key,id])?;
    tx.execute("INSERT INTO feature(layer,id,wkb,attributes,minx,maxx,miny,maxy) VALUES(?1,?2,?3,?4,?5,?6,?7,?8)",params![key,id,row.image.wkb,a,b[0],b[1],b[2],b[3]])?;
   }
   count+=1;
  }
  if count==0{return Err(Error::Invalid)}
  let digest=format!("{:x}",hash.finalize());
  if let Some((old,_,_,n))=existing{
   if old!=digest||n!=count{return Err(Error::Reused)}
  }else{
   tx.execute("INSERT INTO bulk_import VALUES(?1,?2,?3,?4,?5)",params![op,digest,key,revision,count])?;
  }
  if cancel.load(Ordering::Acquire){return Err(Error::Conflict)}
  tx.commit()?;Ok(BulkReceipt{layer,revision,count,digest})
 }
}
