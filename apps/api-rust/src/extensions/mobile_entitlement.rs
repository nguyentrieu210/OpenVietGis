//! Candidate issuer boundary; not an HTTP endpoint or payment service.
//! Caller must resolve current entitlement/activation and permissions from the
//! server-owned registry in a reviewed transaction. Never issue from request JSON.
use serde::{Serialize,Deserialize};
use uuid::Uuid;
use ed25519_dalek::{SigningKey,Signer};
use base64::{engine::general_purpose::STANDARD,Engine};
use serde_json::{json,Value};
use super::mobile_v2::canonical;
#[derive(Clone,Debug,Serialize,Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Entitlement{
 pub id:Uuid,pub organization:Uuid,pub manifest_id:Uuid,pub version:String,
 pub release_digest:String,pub state:String,pub not_before:i64,pub expires_at:i64,pub revision:u64
}
pub struct Subject<'a>{
 pub issuer:&'a str,pub organization:Uuid,pub project:Uuid,pub user:Uuid,pub device:Uuid,pub session:Uuid,
 pub active_manifest:Uuid,pub release_digest:&'a str,pub epoch:u64,pub active:bool
}
fn digest(s:&str)->bool{s.len()==64&&s.bytes().all(|b|matches!(b,b'0'..=b'9'|b'a'..=b'f'))}
/// Explicit offline grants only. Actual data permission/lease is checked
/// independently by the capability broker; this token is not auth.
pub fn issue(ent:&Entitlement,subject:&Subject<'_>,now:i64,lease_expires:i64,key_id:&str,signer:&SigningKey)->Result<Value,()>{
 if !subject.active||ent.state!="active"||ent.organization!=subject.organization||ent.manifest_id!=subject.active_manifest||ent.release_digest!=subject.release_digest||!digest(&ent.release_digest)||ent.not_before>now||now<1||ent.expires_at<=now||lease_expires<=now||ent.revision<1||subject.epoch<1||subject.issuer.is_empty()||key_id.is_empty()||[ent.id,ent.organization,ent.manifest_id,subject.project,subject.user,subject.device,subject.session].iter().any(Uuid::is_nil){return Err(());}
 let expiry=ent.expires_at.min(lease_expires).min(now.checked_add(86400).ok_or(())?);
 let payload=json!({"schema_version":1,"issuer":subject.issuer,"entitlement_id":ent.id,"organization":ent.organization,"project":subject.project,"subject":subject.user,"device":subject.device,"session":subject.session,"manifest_id":ent.manifest_id,"version":ent.version,"release_digest":ent.release_digest,"epoch":subject.epoch,"revision":ent.revision,"issued_at":now,"expires_at":expiry,"key_id":key_id});
 let signature=STANDARD.encode(signer.sign(&canonical(&payload)?).to_bytes());
 Ok(json!({"payload":payload,"signature":signature}))
}
#[cfg(test)]
mod tests{
 use super::*;
 #[test]fn expiry_tenant_and_revoked(){
  let id=Uuid::new_v4();let org=Uuid::new_v4();let hash="a".repeat(64);
  let mut e=Entitlement{id:Uuid::new_v4(),organization:org,manifest_id:id,version:"1.0.0".into(),release_digest:hash.clone(),state:"active".into(),not_before:10,expires_at:100000,revision:1};
  let mut s=Subject{issuer:"fixture",organization:org,project:Uuid::new_v4(),user:Uuid::new_v4(),device:Uuid::new_v4(),session:Uuid::new_v4(),active_manifest:id,release_digest:&hash,epoch:3,active:true};
  let k=SigningKey::from_bytes(&[9;32]);let a=issue(&e,&s,100,200,"fixture",&k).unwrap();assert_eq!(a["payload"]["expires_at"],200);
  for status in ["unpaid","revoked","expired"]{e.state=status.into();assert!(issue(&e,&s,100,200,"fixture",&k).is_err());}
  e.state="active".into();s.organization=Uuid::new_v4();assert!(issue(&e,&s,100,200,"fixture",&k).is_err());s.organization=org;
  assert!(issue(&e,&s,100,100,"fixture",&k).is_err());
 }
}
