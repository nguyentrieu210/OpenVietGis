//! Versioned exact-byte signed envelope. No self-issued lease or wall clock trust.
use super::{Scope,Error,rev};
use ed25519_dalek::{Signature,VerifyingKey};
use serde::{Serialize,Deserialize};
#[derive(Clone,Debug,Serialize,Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Claims{pub version:u8,pub scope:Scope,pub key_id:String,pub package_digest:String,pub permission_epoch:i64,pub issued_ms:i64,pub expires_ms:i64,pub can_edit:bool}
#[derive(Clone,Debug)]
pub struct Clock{pub boot_id:String,pub monotonic_ms:u64,pub wall_ms:i64}
pub struct Lease{claims:Claims,anchor:Clock,server_ms:i64}
impl Lease{
 pub fn verify(payload:&[u8],signature:&[u8;64],key:&VerifyingKey,key_id:&str,scope:&Scope,server_ms:i64,anchor:Clock)->Result<Self,Error>{
  if payload.len()>8192||anchor.boot_id.is_empty()||server_ms<=0{return Err(Error::Invalid);}
  key.verify_strict(payload,&Signature::from_bytes(signature)).map_err(|_|Error::Locked)?;
  let c:Claims=serde_json::from_slice(payload).map_err(|_|Error::Invalid)?;
  scope.validate()?;
  if &c.scope!=scope||c.key_id!=key_id{return Err(Error::Scope);}
  if c.version!=1||!rev(&c.package_digest)||c.permission_epoch<1||c.issued_ms<=0||c.issued_ms>server_ms||c.expires_ms<=server_ms||c.expires_ms.checked_sub(c.issued_ms).is_none_or(|n|n>86400000){return Err(Error::Locked);}
  Ok(Self{claims:c,anchor,server_ms})
 }
 pub fn check(&self,clock:&Clock,scope:&Scope,epoch:i64,edit:bool)->Result<(),Error>{
  if &self.claims.scope!=scope||epoch!=self.claims.permission_epoch{return Err(Error::Scope);}
  let elapsed=clock.monotonic_ms.checked_sub(self.anchor.monotonic_ms).and_then(|n|i64::try_from(n).ok()).ok_or(Error::Locked)?;
  if clock.boot_id!=self.anchor.boot_id||clock.wall_ms<self.anchor.wall_ms||self.server_ms.checked_add(elapsed).is_none_or(|t|t>=self.claims.expires_ms)|| (edit&&!self.claims.can_edit){return Err(Error::Locked);}Ok(())
 }
 pub fn claims(&self)->&Claims{&self.claims}
}
