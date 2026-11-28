//! Trusted-native encrypted attachment migration. No key in SQLite, JSON,
//! command-line arguments or Flutter IPC. A platform key provider must authorize
//! each domain; no fallback key and no plaintext staging file.
use super::*;
use chacha20poly1305::{XChaCha20Poly1305,XNonce,KeyInit,aead::{Aead,Payload,OsRng,rand_core::RngCore}};
use zeroize::Zeroizing;
use ovg_local_storage::media::{Vault,BlobRef};
use std::sync::atomic::{AtomicBool,Ordering};
const MAGIC:&[u8;8]=b"OVGMED01";
const MAX:usize=16*1024*1024;
const TOTAL:u64=512*1024*1024;
const COUNT:u64=4096;
/// Implemented by the trusted native host, not by a renderer. Missing key
/// returns an error; never generate a replacement for an existing domain.
pub trait MediaKeys{fn key_for(&self,scope:Uuid)->Result<Zeroizing<[u8;32]>>;}
#[derive(Clone,Debug)]
pub struct ImportRef{pub layer:Uuid,pub feature:Uuid,pub blob:BlobRef}
#[derive(Clone,Debug,Serialize,PartialEq,Eq)]
pub struct MediaMeta{pub id:Uuid,pub scope:Uuid,pub layer:Uuid,pub feature:Uuid,pub source_scope:Uuid,pub source_id:Uuid,pub source_sha:[u8;32],pub cipher_sha:[u8;32],pub cipher_bytes:u64}
fn uid(s:&str)->Result<Uuid>{let u=Uuid::parse_str(s).map_err(|_|Error::Invalid)?;if u.is_nil()||u.to_string()!=s{return Err(Error::Invalid)}Ok(u)}
fn key(keys:&impl MediaKeys,scope:Uuid)->Result<Zeroizing<[u8;32]>>{
 let k=keys.key_for(scope)?;if k.iter().all(|&b|b==0){return Err(Error::Invalid)}Ok(k)
}
fn aad(m:&MediaMeta)->Vec<u8>{
 let mut a=MAGIC.to_vec();
 for u in [m.scope,m.id,m.layer,m.feature,m.source_scope,m.source_id]{a.extend(u.as_bytes());}
 a.extend(m.source_sha);a
}
fn domain(db:&Connection)->Result<Option<Uuid>>{
 let d:Option<String>=db.query_row("SELECT scope FROM media_domain WHERE singleton=1",[],|r|r.get(0)).optional()?;
 d.map(|s|uid(&s)).transpose()
}
fn meta(r:&rusqlite::Row<'_>)->Result<MediaMeta>{
 let sha=|i|->Result<[u8;32]>{r.get::<_,Vec<u8>>(i)?.try_into().map_err(|_|Error::Invalid)};
 Ok(MediaMeta{id:uid(&r.get::<_,String>(0)?)?,scope:uid(&r.get::<_,String>(1)?)?,layer:uid(&r.get::<_,String>(2)?)?,feature:uid(&r.get::<_,String>(3)?)?,source_scope:uid(&r.get::<_,String>(4)?)?,source_id:uid(&r.get::<_,String>(5)?)?,source_sha:sha(6)?,cipher_sha:sha(7)?,cipher_bytes:r.get(8)?})
}
fn valid(m:&MediaMeta,data:&[u8])->bool{
 data.len()>48&&data.len()<=MAX+48&&data.len() as u64==m.cipher_bytes&&data.starts_with(MAGIC)&&<[u8;32]>::from(Sha256::digest(data))==m.cipher_sha
}
fn capture_ref(scope:Uuid,id:Uuid)->[u8;32]{
 let mut h=Sha256::new();h.update(b"OVG-NATIVE-CAPTURE-1");h.update(scope.as_bytes());h.update(id.as_bytes());h.finalize().into()
}
fn jpeg(data:&[u8])->bool{data.len()>=4&&data.len()<=MAX&&data.starts_with(&[0xff,0xd8,0xff])&&data.ends_with(&[0xff,0xd9])}
const COLS:&str="id,scope,layer,feature,source_scope,source_id,source_sha,cipher_sha,cipher_bytes";
impl Store{
 /// Only a trusted native camera supplies this buffer. No plaintext staging.
 /// JPEG framing check is not an image decoder; the platform must validate
 /// dimensions/decodability before presenting media.
 pub fn capture_jpeg(&self,scope:Uuid,layer:Uuid,feature:Uuid,id:Uuid,data:&[u8],expected:u64,keys:&impl MediaKeys)->Result<(u64,MediaMeta)>{
  if scope.is_nil()||id.is_nil()||!jpeg(data){return Err(Error::Invalid)}
  let k=key(keys,scope)?;let c=XChaCha20Poly1305::new_from_slice(&k[..]).map_err(|_|Error::Invalid)?;
  let mut db=self.db.lock().map_err(lock_err)?;let tx=db.transaction_with_behavior(TransactionBehavior::Immediate)?;
  if current(&tx,layer,feature)?.1.is_none(){return Err(Error::Invalid)}
  match domain(&tx)?{Some(s) if s!=scope=>return Err(Error::Conflict),None=>{tx.execute("INSERT INTO media_domain VALUES(1,?1)",[scope.to_string()])?;},_=>{}}
  let(count,used):(u64,u64)=tx.query_row("SELECT count(*),coalesce(sum(cipher_bytes),0) FROM encrypted_media",[],|r|Ok((r.get(0)?,r.get(1)?)))?;
  if count>=COUNT||used.saturating_add(data.len()as u64+48)>TOTAL{return Err(Error::Limit)}
  if tx.query_row("SELECT EXISTS(SELECT 1 FROM encrypted_media WHERE id=?1)",[id.to_string()],|r|r.get::<_,bool>(0))?{return Err(Error::Reused)}
  let revision=next(&tx,expected)?;
  let mut m=MediaMeta{id,scope,layer,feature,source_scope:scope,source_id:id,source_sha:capture_ref(scope,id),cipher_sha:[0;32],cipher_bytes:0};
  let mut nonce=[0;24];OsRng.fill_bytes(&mut nonce);
  let body=c.encrypt(XNonce::from_slice(&nonce),Payload{msg:data,aad:&aad(&m)}).map_err(|_|Error::Invalid)?;
  let mut encrypted=MAGIC.to_vec();encrypted.extend(nonce);encrypted.extend(body);
  m.cipher_sha=Sha256::digest(&encrypted).into();m.cipher_bytes=encrypted.len()as u64;
  tx.execute("INSERT INTO encrypted_media VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10)",params![id.to_string(),scope.to_string(),layer.to_string(),feature.to_string(),scope.to_string(),id.to_string(),m.source_sha.as_slice(),m.cipher_sha.as_slice(),m.cipher_bytes,encrypted])?;
  tx.execute("INSERT INTO media_capture VALUES(?1,'image/jpeg')",[id.to_string()])?;
  tx.commit()?;Ok((revision,m))
 }
 pub fn media_domain(&self)->Result<Option<Uuid>>{domain(&*self.db.lock().map_err(lock_err)?)}
 /// Import an explicitly authorized list of legacy references into a distinct
 /// destination key domain. The caller provisions that domain in secure key
 /// storage before invoking this method. All rows commit together or none do.
 pub fn import_encrypted_media(&self,source_root:&Path,source_scope:Uuid,destination_scope:Uuid,refs:&[ImportRef],keys:&impl MediaKeys,expected:u64,cancel:&AtomicBool)->Result<(u64,Vec<MediaMeta>)>{
  if source_scope.is_nil()||destination_scope.is_nil()||source_scope==destination_scope||refs.is_empty()||refs.len()>16{return Err(Error::Invalid)}
  if !std::fs::symlink_metadata(source_root).map_err(|_|Error::Storage)?.file_type().is_dir(){return Err(Error::Invalid)}
  let source_key=key(keys,source_scope)?;let dest_key=key(keys,destination_scope)?;
  if source_key[..]==dest_key[..]{return Err(Error::Invalid)}
  let vault=Vault::open(source_root,source_scope,source_key).map_err(|_|Error::Invalid)?;
  let cipher=XChaCha20Poly1305::new_from_slice(dest_key.as_ref()).map_err(|_|Error::Invalid)?;
  let mut db=self.db.lock().map_err(lock_err)?;let tx=db.transaction_with_behavior(TransactionBehavior::Immediate)?;
  match domain(&tx)?{Some(d) if d!=destination_scope=>return Err(Error::Conflict),None=>{tx.execute("INSERT INTO media_domain VALUES(1,?1)",[destination_scope.to_string()])?;},_=>{}}
  let (count,mut used):(u64,u64)=tx.query_row("SELECT count(*),coalesce(sum(cipher_bytes),0) FROM encrypted_media",[],|r|Ok((r.get(0)?,r.get(1)?)))?;
  if count+refs.len()as u64>COUNT{return Err(Error::Limit)}
  let revision=next(&tx,expected)?;
  let mut ids=HashSet::new();let mut out=Vec::new();
  for r in refs{
   if cancel.load(Ordering::Acquire){return Err(Error::Conflict)}
   if r.blob.id.is_nil()||!ids.insert(r.blob.id)||current(&tx,r.layer,r.feature)?.1.is_none(){return Err(Error::Invalid)}
   if tx.query_row("SELECT EXISTS(SELECT 1 FROM encrypted_media WHERE source_scope=?1 AND source_id=?2)",params![source_scope.to_string(),r.blob.id.to_string()],|r|r.get::<_,bool>(0))?{return Err(Error::Reused)}
   // Vault verifies SHA256 and source AAD before returning zeroizing plaintext.
   let plain=vault.read(&r.blob).map_err(|_|Error::Invalid)?;
   if plain.is_empty()||plain.len()>MAX{return Err(Error::Limit)}
   let mut m=MediaMeta{id:Uuid::new_v4(),scope:destination_scope,layer:r.layer,feature:r.feature,source_scope,source_id:r.blob.id,source_sha:r.blob.ciphertext_sha256,cipher_sha:[0;32],cipher_bytes:0};
   let mut nonce=[0u8;24];OsRng.fill_bytes(&mut nonce);
   let body=cipher.encrypt(XNonce::from_slice(&nonce),Payload{msg:plain.as_ref(),aad:&aad(&m)}).map_err(|_|Error::Invalid)?;
   drop(plain);
   let mut data=MAGIC.to_vec();data.extend(nonce);data.extend(body);
   m.cipher_sha=Sha256::digest(&data).into();m.cipher_bytes=data.len()as u64;
   used=used.checked_add(m.cipher_bytes).ok_or(Error::Limit)?;if used>TOTAL{return Err(Error::Limit)}
   tx.execute("INSERT INTO encrypted_media VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10)",params![m.id.to_string(),m.scope.to_string(),m.layer.to_string(),m.feature.to_string(),m.source_scope.to_string(),m.source_id.to_string(),m.source_sha.as_slice(),m.cipher_sha.as_slice(),m.cipher_bytes,data])?;
   out.push(m);
  }
  if cancel.load(Ordering::Acquire){return Err(Error::Conflict)}
  tx.commit()?;Ok((revision,out))
 }
 /// Metadata remains visible while locked. Tombstoned owners remain archived;
 /// the attachment must not disappear when a feature is deleted or undone.
 pub fn media_list(&self,layer:Uuid,after:&str,limit:u32,expected:u64)->Result<Vec<MediaMeta>>{
  if !(1..=100).contains(&limit)||!after.is_empty()&&uid(after).is_err(){return Err(Error::Invalid)}
  let db=self.db.lock().map_err(lock_err)?;if rev(&db)?!=expected{return Err(Error::Conflict)}known_layer(&db,layer)?;
  let mut q=db.prepare(&format!("SELECT {COLS} FROM encrypted_media WHERE layer=?1 AND id>?2 ORDER BY id LIMIT ?3"))?;
  let mut rows=q.query(params![layer.to_string(),after,limit])?;let mut out=Vec::new();while let Some(r)=rows.next()?{out.push(meta(r)?)}Ok(out)
 }
 /// Trusted native consumer only. No filesystem materialization or renderer
 /// key export. A portable backup preserves the same domain ID.
 pub fn read_encrypted_media(&self,id:Uuid,expected:u64,keys:&impl MediaKeys)->Result<Zeroizing<Vec<u8>>>{
  let db=self.db.lock().map_err(lock_err)?;if rev(&db)?!=expected{return Err(Error::Conflict)}
  let mut q=db.prepare(&format!("SELECT {COLS},ciphertext FROM encrypted_media WHERE id=?1 AND length(ciphertext)<=?2"))?;
  let mut rows=q.query(params![id.to_string(),(MAX+48)as u64])?;let row=rows.next()?.ok_or(Error::Invalid)?;
  let m=meta(row)?;let data:Vec<u8>=row.get(9)?;
  if domain(&db)?!=Some(m.scope)||!valid(&m,&data){return Err(Error::Invalid)}
  let k=key(keys,m.scope)?;let c=XChaCha20Poly1305::new_from_slice(k.as_ref()).map_err(|_|Error::Invalid)?;
  let plain=Zeroizing::new(c.decrypt(XNonce::from_slice(&data[8..32]),Payload{msg:&data[32..],aad:&aad(&m)}).map_err(|_|Error::Invalid)?);
  if plain.is_empty()||plain.len()>MAX{return Err(Error::Invalid)}Ok(plain)
 }
}
/// Keyless integrity checks storage shape and ciphertext hash, not AEAD tags.
pub(crate) fn verify(db:&Connection)->Result<bool>{
 let scope=domain(db)?;let (count,bytes):(u64,u64)=db.query_row("SELECT count(*),coalesce(sum(cipher_bytes),0) FROM encrypted_media",[],|r|Ok((r.get(0)?,r.get(1)?)))?;
 if count>COUNT||bytes>TOTAL{return Ok(false)}
 let mut q=db.prepare(&format!("SELECT {COLS},ciphertext FROM encrypted_media WHERE length(ciphertext)<=?1 ORDER BY id"))?;
 let mut rows=q.query([(MAX+48)as u64])?;let mut n=0;
 while let Some(r)=rows.next()?{let m=meta(r)?;let b:Vec<u8>=r.get(9)?;let captured:bool=db.query_row("SELECT EXISTS(SELECT 1 FROM media_capture WHERE id=?1)",[m.id.to_string()],|r|r.get(0))?;
  if scope!=Some(m.scope)||!valid(&m,&b){return Ok(false)}
  if captured{if m.source_scope!=m.scope||m.source_id!=m.id||m.source_sha!=capture_ref(m.scope,m.id){return Ok(false)}}
  else if m.source_scope==m.scope{return Ok(false)}
  n+=1;}
 Ok(n==count)
}
