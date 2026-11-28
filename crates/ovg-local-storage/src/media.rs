//! Candidate encrypted local attachment vault. Only a trusted platform may supply
//! a scoped unwrapped key. NOT an encrypted managed SQLite database or keystore.
use super::Error;
use chacha20poly1305::{XChaCha20Poly1305,XNonce,KeyInit,aead::{Aead,Payload,OsRng,rand_core::RngCore}};
use sha2::{Sha256,Digest};
use zeroize::Zeroizing;
use std::{fs::{self,File,OpenOptions},io::{Read,Write},path::{Path,PathBuf}};
use uuid::Uuid;
const LIMIT:usize=16*1024*1024;
const MAGIC:&[u8;8]=b"OVGBLOB1";
pub struct Vault{root:PathBuf,scope:Uuid,key:Zeroizing<[u8;32]>}
#[derive(Debug,Clone)]
pub struct BlobRef{pub id:Uuid,pub ciphertext_sha256:[u8;32]}
impl Vault{
 pub fn open(root:&Path,scope:Uuid,key:Zeroizing<[u8;32]>)->Result<Self,Error>{
   if scope.is_nil()||!root.is_absolute()||key.iter().all(|b|*b==0){return Err(Error::Invalid);}
   fs::create_dir_all(root)?;
   if !fs::symlink_metadata(root)?.file_type().is_dir(){return Err(Error::Invalid);}
   Ok(Self{root:root.canonicalize()?,scope,key})
 }
 fn aad(&self,id:Uuid)->Vec<u8>{let mut a=MAGIC.to_vec();a.extend(self.scope.as_bytes());a.extend(id.as_bytes());a}
 fn path(&self,id:Uuid)->PathBuf{self.root.join(format!("{id}.enc"))}
 pub fn put(&self,plaintext:&[u8])->Result<BlobRef,Error>{
   if plaintext.is_empty()||plaintext.len()>LIMIT{return Err(Error::Limit);}
   let id=Uuid::new_v4();let mut nonce=[0u8;24];OsRng.fill_bytes(&mut nonce);
   let cipher=XChaCha20Poly1305::new_from_slice(self.key.as_ref()).map_err(|_|Error::Invalid)?;
   let encrypted=cipher.encrypt(XNonce::from_slice(&nonce),Payload{msg:plaintext,aad:&self.aad(id)}).map_err(|_|Error::Invalid)?;
   let mut blob=MAGIC.to_vec();blob.extend(nonce);blob.extend(encrypted);
   let hash: [u8;32]=Sha256::digest(&blob).into();
   let tmp=self.root.join(format!(".{id}.staging"));
   let result=(||->Result<(),Error>{
      let mut f=OpenOptions::new().write(true).create_new(true).open(&tmp)?;
      f.write_all(&blob)?;f.sync_all()?;drop(f);
      fs::hard_link(&tmp,self.path(id))?;
      File::open(&self.root)?.sync_all()?;Ok(())
   })();
   let _=fs::remove_file(&tmp);result?;
   Ok(BlobRef{id,ciphertext_sha256:hash})
 }
 pub fn read(&self,reference:&BlobRef)->Result<Zeroizing<Vec<u8>>,Error>{
   if reference.id.is_nil(){return Err(Error::Invalid);}
   let path=self.path(reference.id);
   let m=fs::symlink_metadata(&path)?;
   if !m.file_type().is_file()||m.len()<48||m.len()>(LIMIT+48)as u64{return Err(Error::Invalid);}
   let mut bytes=Vec::new();File::open(path)?.take((LIMIT+49)as u64).read_to_end(&mut bytes)?;
   if bytes.len()<48||bytes.len()>LIMIT+48||&bytes[..8]!=MAGIC||<[u8;32]>::from(Sha256::digest(&bytes))!=reference.ciphertext_sha256{return Err(Error::Invalid);}
   let cipher=XChaCha20Poly1305::new_from_slice(self.key.as_ref()).map_err(|_|Error::Invalid)?;
   let plaintext=cipher.decrypt(XNonce::from_slice(&bytes[8..32]),Payload{msg:&bytes[32..],aad:&self.aad(reference.id)}).map_err(|_|Error::Invalid)?;
   Ok(Zeroizing::new(plaintext))
 }
 /// Only call while holding exclusive workspace ownership after reconciling
 /// committed references. Unknown files are not deleted.
 pub fn cleanup_staging(&self)->Result<usize,Error>{
   let mut count=0;
   for item in fs::read_dir(&self.root)?{let item=item?;
     let name=item.file_name();let Some(s)=name.to_str()else{continue};
     if s.strip_prefix('.').and_then(|x|x.strip_suffix(".staging")).is_some_and(|x|Uuid::parse_str(x).is_ok())&&item.file_type()?.is_file(){fs::remove_file(item.path())?;count+=1;}
   }
   File::open(&self.root)?.sync_all()?;Ok(count)
 }
}
/// QR is only a bounded opaque text payload; do not open URLs or execute commands.
pub fn qr_text(value:&str)->Result<&str,Error>{
 if value.is_empty()||value.len()>4096||value.chars().any(char::is_control){return Err(Error::Invalid);}Ok(value)
}
