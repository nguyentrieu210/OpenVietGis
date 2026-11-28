//! Isolated SQLCipher candidate. Never opens a standalone GeoPackage.
//! Trusted host supplies verified scope, signing key and OS-unwrapped key.
use serde::{Deserialize,Serialize};
use serde_json::Value;
use sha2::{Digest,Sha256};
use uuid::Uuid;
pub mod lease;
pub mod sync;
pub mod store;
#[derive(Debug,thiserror::Error)]
pub enum Error{
 #[error("INVALID")] Invalid,
 #[error("LOCKED")] Locked,
 #[error("SCOPE_MISMATCH")] Scope,
 #[error("RESET_REQUIRED")] Reset,
 #[error("CONFLICT")] Conflict,
 #[error("UNKNOWN_OUTCOME")] Unknown,
 #[error("RESOURCE_LIMIT")] Limit,
 #[error("STORAGE_ERROR")] Db(#[from] rusqlite::Error),
 #[error("IO_ERROR")] Io(#[from] std::io::Error)
}
#[derive(Clone,Debug,PartialEq,Eq,Serialize,Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Scope{pub origin:String,pub issuer:String,pub subject:Uuid,pub organization:Uuid,pub project:Uuid,pub device:Uuid,pub session:Uuid}
impl Scope{
 pub fn validate(&self)->Result<(),Error>{
  let url=url::Url::parse(&self.origin).map_err(|_|Error::Invalid)?;
  if url.scheme()!="https"||url.host_str().is_none()||url.origin().ascii_serialization()!=self.origin||!self.origin.starts_with("https://")||self.origin.len()>512||self.issuer.is_empty()||self.issuer.len()>256||self.origin[8..].is_empty()||self.origin[8..].contains(['/', '@','?','#','\\'])||[self.subject,self.organization,self.project,self.device,self.session].iter().any(Uuid::is_nil){return Err(Error::Invalid);}Ok(())
 }
 pub fn digest(&self)->Result<[u8;32],Error>{self.validate()?;Ok(Sha256::digest(serde_json::to_vec(self).map_err(|_|Error::Invalid)?).into())}
}
pub(crate) fn hash(data:&[u8])->[u8;32]{Sha256::digest(data).into()}
pub(crate) fn rev(s:&str)->bool{s.len()==64&&s.bytes().all(|c|c.is_ascii_hexdigit())}
pub(crate) fn feature_id(s:&str)->bool{!s.is_empty()&&s.len()<=128&&s.bytes().all(|c|c.is_ascii_alphanumeric()||b"_-".contains(&c))}
pub(crate) fn json_bytes(v:&Value)->Result<Vec<u8>,Error>{serde_json::to_vec(v).map_err(|_|Error::Invalid)}
