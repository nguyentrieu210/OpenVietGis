//! Additive candidate verifier; no change to v1 or mounted catalog routes.
//! Canonical profile: sorted ASCII keys, UTF-8 strings, safe integers only.
//! Operator approves exact release digest and publisher key. Trust anchors and
//! persisted epoch/revocations must come from the host, not the package.
use super::validation::{marketplace_canonical,decode_ed25519_public_key};
use serde::{Deserialize,Serialize};
use serde_json::Value;
use sha2::{Sha256,Digest};
use base64::{engine::general_purpose::STANDARD,Engine};
use ed25519_dalek::Signature;
use std::collections::{HashMap,HashSet};
use uuid::Uuid;
#[derive(Debug,Deserialize,Serialize)]
#[serde(deny_unknown_fields)]
pub struct Asset{pub id:String,pub media_type:String,pub size:u64,pub sha256:String}
#[derive(Debug,Deserialize,Serialize)]
#[serde(deny_unknown_fields)]
pub struct Release{
 pub schema_version:u8,pub publisher_id:String,pub extension_key:String,pub version:String,pub manifest_id:Uuid,
 pub manifest_digest:String,pub host_api_min:u32,pub host_api_max:u32,pub platforms:Vec<String>,
 pub capabilities:Vec<String>,pub permissions:Vec<String>,pub contribution_digest:String,pub assets:Vec<Asset>,pub issued_at:i64,pub key_id:String
}
#[derive(Debug,Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Review{pub schema_version:u8,pub issuer:String,pub catalog_epoch:u64,pub issued_at:i64,pub expires_at:i64,pub publisher_key:String,pub publisher_key_id:String,pub release_digest:String,pub revoked:bool}
pub struct Trust<'a>{pub issuer:&'a str,pub operator_key:&'a str,pub epoch_floor:u64,pub now:i64,pub host_api:u32,pub platform:&'a str,pub revoked_keys:&'a HashSet<String>,pub revoked_releases:&'a HashSet<String>}
fn profile(v:&Value,depth:u8)->bool{
 if depth>24{return false;}
 match v{Value::Object(m)=>m.len()<=500&&m.iter().all(|(k,v)|k.len()<=128&&k.bytes().all(|b|b.is_ascii_alphanumeric()||b"_.:-".contains(&b))&&profile(v,depth+1)),
 Value::Array(a)=>a.len()<=500&&a.iter().all(|v|profile(v,depth+1)),
 Value::Number(n)=>n.as_i64().is_some_and(|x|x.unsigned_abs()<=9007199254740991),
 Value::String(s)=>s.len()<=65536&&!s.chars().any(char::is_control),
 _=>true}
}
pub fn canonical(v:&Value)->Result<Vec<u8>,()>{if !profile(v,0){return Err(());}let b=marketplace_canonical(v)?;if b.len()>256*1024{return Err(());}Ok(b)}
pub fn digest(v:&Value)->Result<String,()>{Ok(hex::encode(Sha256::digest(canonical(v)?)))}
fn verify(key:&str,v:&Value,sig:&str)->Result<(),()>{
 let key=decode_ed25519_public_key(key)?;let b=STANDARD.decode(sig).map_err(|_|())?;
 if STANDARD.encode(&b)!=sig{return Err(());}let s=Signature::from_bytes(&b.try_into().map_err(|_|())?);
 key.verify_strict(&canonical(v)?,&s).map_err(|_|())
}
fn token(s:&str)->bool{!s.is_empty()&&s.len()<=128&&s.bytes().all(|b|b.is_ascii_alphanumeric()||b"_.:-".contains(&b))}
fn unique(v:&[String])->bool{v.len()<=64&&v.iter().all(|s|token(s))&&v.iter().collect::<HashSet<_>>().len()==v.len()}
pub fn validate(release:&Value,publisher_sig:&str,review:&Value,operator_sig:&str,manifest:&Value,manifest_id:Uuid,contribution:&Value,assets:&HashMap<String,Vec<u8>>,trust:&Trust<'_>)->Result<Release,()>{
 verify(trust.operator_key,review,operator_sig)?;
 let r:Review=serde_json::from_value(review.clone()).map_err(|_|())?;
 let d=digest(release)?;
 if r.schema_version!=2||r.issuer!=trust.issuer||r.catalog_epoch<trust.epoch_floor||r.revoked||trust.revoked_keys.contains(&r.publisher_key_id)||trust.revoked_releases.contains(&d)||r.release_digest!=d||r.issued_at>trust.now||r.expires_at<=trust.now||r.expires_at.checked_sub(r.issued_at).is_none_or(|n|n>86400||n<=0){return Err(());}
 verify(&r.publisher_key,release,publisher_sig)?;
 let e:Release=serde_json::from_value(release.clone()).map_err(|_|())?;
 if e.schema_version!=2||e.key_id!=r.publisher_key_id||e.issued_at>r.issued_at||e.manifest_id!=manifest_id||manifest_id.is_nil()||e.manifest_digest!=digest(manifest)?||e.contribution_digest!=digest(contribution)?||manifest["extension_key"]!=e.extension_key||manifest["version"]!=e.version||!token(&e.publisher_id)||!token(&e.extension_key)||!token(&e.version)||!token(&e.key_id)||!e.extension_key.starts_with(&format!("partner_{}_",e.publisher_id))||!(e.host_api_min..=e.host_api_max).contains(&trust.host_api)||!unique(&e.platforms)||!e.platforms.iter().all(|s|matches!(s.as_str(),"android"|"ios"))||!e.platforms.iter().any(|s|s==trust.platform)||!unique(&e.permissions)||!unique(&e.capabilities)||e.assets.len()>64||e.assets.len()!=assets.len(){return Err(());}
 let permissions=manifest["permissions"].as_array().ok_or(())?;
 if e.permissions.iter().any(|p|!permissions.iter().any(|v|v.as_str()==Some(p))){return Err(());}
 let mut total=0u64;let mut seen=HashSet::new();
 for a in &e.assets{
  if !token(&a.id)||!seen.insert(&a.id)||!matches!(a.media_type.as_str(),"application/json"|"text/plain"){return Err(());}
  let bytes=assets.get(&a.id).ok_or(())?;total=total.checked_add(a.size).ok_or(())?;
  if total>10*1024*1024||a.size!=bytes.len() as u64||a.sha256!=hex::encode(Sha256::digest(bytes)){return Err(());}
 }Ok(e)
}
#[cfg(test)]
mod tests{
 use super::*;
 #[test]fn independent_v2_fixture_and_revocation(){
  let v:Value=serde_json::from_str(include_str!("../../../../packages/flutter-plugin-sdk/test/fixtures/release-v2.json")).unwrap();
  let keys=HashSet::new();let releases=HashSet::new();
  let mut t=Trust{issuer:"fixture-instance",operator_key:v["operator_key"].as_str().unwrap(),epoch_floor:4,now:1700000100,host_api:2,platform:"android",revoked_keys:&keys,revoked_releases:&releases};
  let id=Uuid::parse_str(v["release"]["manifest_id"].as_str().unwrap()).unwrap();
  let run=|x:&Value,t:&Trust<'_>|validate(&x["release"],x["publisher_signature"].as_str().unwrap(),&x["review"],x["operator_signature"].as_str().unwrap(),&x["manifest"],id,&x["contribution"],&HashMap::new(),t);
  assert!(run(&v,&t).is_ok());let mut bad=v.clone();bad["contribution"]["nodes"][0]["label"]="tampered".into();assert!(run(&bad,&t).is_err());
  t.epoch_floor=5;assert!(run(&v,&t).is_err());t.epoch_floor=4;t.now=1700003600;assert!(run(&v,&t).is_err());
 }
}
