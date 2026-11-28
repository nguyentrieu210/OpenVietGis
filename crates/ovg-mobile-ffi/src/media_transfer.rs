//! Offline, single-use X25519/HKDF/XChaCha20Poly1305 key envelope.
//! Only the public request and encrypted envelope cross FRB. Sender must
//! compare recipient fingerprint out of band; receiving verifies key against
//! an existing authenticated attachment before native installation.
use ovg_project_store::Error;
use chacha20poly1305::{XChaCha20Poly1305,XNonce,KeyInit,aead::{Aead,Payload,OsRng,rand_core::RngCore}};
use hkdf::Hkdf;
use sha2::{Sha256,Digest};
use uuid::Uuid;
use x25519_dalek::{PublicKey,StaticSecret};
use zeroize::Zeroizing;
use std::{sync::Mutex,time::{Duration,Instant}};
const PREFIX:&str="OVG-KEY-TRANSFER-1";
struct Pending{scope:Uuid,project:Uuid,secret:StaticSecret,request:String,deadline:Instant}
static RECEIVER:Mutex<Option<Pending>>=Mutex::new(None);
fn hex(b:&[u8])->String{b.iter().map(|v|format!("{v:02x}")).collect()}
fn unhex<const N:usize>(s:&str)->Result<[u8;N],Error>{
 if s.len()!=N*2||!s.bytes().all(|v|v.is_ascii_digit()||(b'a'..=b'f').contains(&v)){return Err(Error::Invalid)}
 let mut b=[0;N];for(i,v)in b.iter_mut().enumerate(){*v=u8::from_str_radix(&s[i*2..i*2+2],16).map_err(|_|Error::Invalid)?;}Ok(b)
}
fn request(s:&str)->Result<(Uuid,[u8;32]),Error>{
 if s.len()>160{return Err(Error::Limit)}
 let mut p=s.split(':');if p.next()!=Some(PREFIX){return Err(Error::Invalid)}
 let id=p.next().ok_or(Error::Invalid)?;let scope=Uuid::parse_str(id).map_err(|_|Error::Invalid)?;
 if scope.is_nil()||scope.to_string()!=id{return Err(Error::Invalid)}
 let pk=unhex::<32>(p.next().ok_or(Error::Invalid)?)?;
 let _=unhex::<16>(p.next().ok_or(Error::Invalid)?)?;
 if p.next().is_some()||pk==[0;32]{return Err(Error::Invalid)}Ok((scope,pk))
}
pub fn fingerprint(s:&str)->Result<String,Error>{request(s)?;Ok(hex(&Sha256::digest(s.as_bytes())))}
fn derive(secret:&StaticSecret,pk:[u8;32],request:&str)->Result<Zeroizing<[u8;32]>,Error>{
 let shared=secret.diffie_hellman(&PublicKey::from(pk));if !shared.was_contributory(){return Err(Error::Invalid)}
 let h=Sha256::digest(request.as_bytes());let mut key=Zeroizing::new([0;32]);
 Hkdf::<Sha256>::new(Some(&h),shared.as_bytes()).expand(b"OVG media scope key envelope v1",&mut key[..]).map_err(|_|Error::Invalid)?;Ok(key)
}
pub fn begin(project:Uuid,scope:Uuid)->Result<String,Error>{
 if project.is_nil()||scope.is_nil(){return Err(Error::Invalid)}
 let mut state=RECEIVER.lock().map_err(|_|Error::Storage)?;
 let secret=StaticSecret::random_from_rng(OsRng);let public=PublicKey::from(&secret);
 let mut challenge=[0;16];OsRng.fill_bytes(&mut challenge);
 let req=format!("{PREFIX}:{scope}:{}:{}",hex(public.as_bytes()),hex(&challenge));
 *state=Some(Pending{project,scope,secret,request:req.clone(),deadline:Instant::now()+Duration::from_secs(300)});
 Ok(req)
}
pub fn cancel(project:Uuid){if let Ok(mut s)=RECEIVER.lock(){if s.as_ref().is_some_and(|p|p.project==project){*s=None;}}}
pub fn seal(scope:Uuid,req:&str,confirmed:&str,key:&[u8;32])->Result<String,Error>{
 let(s,pk)=request(req)?;
 if s!=scope||fingerprint(req)?!=confirmed||key==&[0;32]{return Err(Error::Invalid)}
 let secret=StaticSecret::random_from_rng(OsRng);let public=PublicKey::from(&secret);
 let wrapping=derive(&secret,pk,req)?;
 let cipher=XChaCha20Poly1305::new_from_slice(&wrapping[..]).map_err(|_|Error::Invalid)?;
 let mut nonce=[0;24];OsRng.fill_bytes(&mut nonce);
 let mut aad=req.as_bytes().to_vec();aad.extend(public.as_bytes());
 let encrypted=cipher.encrypt(XNonce::from_slice(&nonce),Payload{msg:key,aad:&aad}).map_err(|_|Error::Invalid)?;
 Ok(format!("{PREFIX}:{}:{}:{}:{}",fingerprint(req)?,hex(public.as_bytes()),hex(&nonce),hex(&encrypted)))
}
pub fn open(project:Uuid,scope:Uuid,envelope:&str)->Result<Zeroizing<[u8;32]>,Error>{
 if envelope.len()>320{return Err(Error::Limit)}
 let mut s=RECEIVER.lock().map_err(|_|Error::Storage)?;
 if !s.as_ref().is_some_and(|p|p.project==project&&p.scope==scope){return Err(Error::Conflict)}
 let pending=s.take().ok_or(Error::Conflict)?;drop(s);
 if Instant::now()>pending.deadline{return Err(Error::Conflict)}
 let mut fields=envelope.split(':');
 if fields.next()!=Some(PREFIX)||fields.next()!=Some(fingerprint(&pending.request)?.as_str()){return Err(Error::Invalid)}
 let pk=unhex::<32>(fields.next().ok_or(Error::Invalid)?)?;
 let nonce=unhex::<24>(fields.next().ok_or(Error::Invalid)?)?;
 let encrypted=unhex::<48>(fields.next().ok_or(Error::Invalid)?)?;
 if fields.next().is_some(){return Err(Error::Invalid)}
 let key=derive(&pending.secret,pk,&pending.request)?;
 let cipher=XChaCha20Poly1305::new_from_slice(&key[..]).map_err(|_|Error::Invalid)?;
 let mut aad=pending.request.as_bytes().to_vec();aad.extend(pk);
 let raw=Zeroizing::new(cipher.decrypt(XNonce::from_slice(&nonce),Payload{msg:&encrypted,aad:&aad}).map_err(|_|Error::Invalid)?);
 let mut out=Zeroizing::new([0;32]);if raw.len()!=32||raw.iter().all(|&v|v==0){return Err(Error::Invalid)}
 out.copy_from_slice(&raw);Ok(out)
}
#[cfg(test)]mod tests{
 use super::*;
 #[test]fn roundtrip_and_replay(){
  let p=Uuid::new_v4();let s=Uuid::new_v4();let req=begin(p,s).unwrap();
  let fp=fingerprint(&req).unwrap();assert!(seal(s,&req,"wrong",&[7;32]).is_err());
  let env=seal(s,&req,&fp,&[7;32]).unwrap();
  assert!(open(Uuid::new_v4(),s,&env).is_err());
  assert_eq!(*open(p,s,&env).unwrap(),[7;32]);assert!(open(p,s,&env).is_err());
  let req=begin(p,s).unwrap();let mut env=seal(s,&req,&fingerprint(&req).unwrap(),&[7;32]).unwrap();let last=env.pop().unwrap();env.push(if last=='0'{'1'}else{'0'});
  assert!(open(p,s,&env).is_err());assert!(open(p,s,&env).is_err());
 }
}
