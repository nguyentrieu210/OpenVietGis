//! Candidate additive human-mobile credential repository. Not mounted into HTTP
//! or accepted by existing browser/service extractors. Requires reviewed schema
//! and server-side public-client configuration; no client-supplied user ID.
use crate::{DbPool,identity::{VerifiedSession,random_urlsafe_token}};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD,Engine};
use sha2::{Sha256,Digest};
use subtle::ConstantTimeEq;
use sqlx::Row;
use uuid::Uuid;
use chrono::{Utc,Duration};
#[derive(Debug)]
pub enum Error{Invalid,Revoked,Unavailable}
impl From<sqlx::Error> for Error{fn from(_:sqlx::Error)->Self{Self::Unavailable}}
#[derive(Clone)]
pub struct Client{issuer:String,audience:String,redirect:String}
impl Client{
 pub fn reviewed(issuer:&str,audience:&str,redirect:&str)->Result<Self,Error>{
  let u=url::Url::parse(issuer).map_err(|_|Error::Invalid)?;
  let r=url::Url::parse(redirect).map_err(|_|Error::Invalid)?;
  if u.scheme()!="https"||r.scheme()!="https"||u.host_str().is_none()||r.host_str().is_none()||u.fragment().is_some()||r.fragment().is_some()||u.query().is_some()||r.query().is_some()||!u.username().is_empty()||!r.username().is_empty()||u.password().is_some()||r.password().is_some()||audience.is_empty()||audience.len()>128||issuer.len()>512||redirect.len()>512{return Err(Error::Invalid);}
  Ok(Self{issuer:issuer.into(),audience:audience.into(),redirect:redirect.into()})
 }
}
fn digest(s:&str)->[u8;32]{Sha256::digest(s.as_bytes()).into()}
fn verifier_ok(s:&str)->bool{(43..=128).contains(&s.len())&&s.bytes().all(|b|b.is_ascii_alphanumeric()||b"-._~".contains(&b))}
pub fn pkce_matches(verifier:&str,challenge:&str)->bool{
 verifier_ok(verifier)&&challenge.len()==43&&bool::from(URL_SAFE_NO_PAD.encode(digest(verifier)).as_bytes().ct_eq(challenge.as_bytes()))
}
fn token(prefix:&str)->Result<String,Error>{Ok(format!("{prefix}{}",random_urlsafe_token(32).map_err(|_|Error::Unavailable)?))}
fn parse(raw:&str,prefix:&str)->Result<[u8;32],Error>{
 let s=raw.strip_prefix(prefix).ok_or(Error::Invalid)?;
 if s.len()!=43||URL_SAFE_NO_PAD.decode(s).ok().is_none_or(|v|v.len()!=32){return Err(Error::Invalid);}Ok(digest(raw))
}
/// A dedicated grant may only be created after existing human authentication,
/// browser consent, CSRF verification, and upstream OIDC nonce validation.
pub async fn issue_grant(pool:&DbPool,human:&VerifiedSession,client:&Client,redirect:&str,challenge:&str,device:Uuid)->Result<String,Error>{
 if redirect!=client.redirect||device.is_nil()||challenge.len()!=43||URL_SAFE_NO_PAD.decode(challenge).ok().is_none_or(|v|v.len()!=32){return Err(Error::Invalid);}
 let code=token("ovg_mc_")?;
 let result=sqlx::query("INSERT INTO identity.mobile_grants(code_hash,web_session_id,user_id,issuer,audience,redirect_uri,challenge,device_id,expires_at) SELECT $1,s.id,s.user_id,$2,$3,$4,$5,$6,clock_timestamp()+interval '5 minutes' FROM identity.web_sessions s JOIN identity.users u ON u.id=s.user_id JOIN identity.identity_links l ON l.id=s.identity_link_id AND l.user_id=u.id WHERE s.id=$7 AND s.user_id=$8 AND s.revoked_at IS NULL AND s.expires_at>clock_timestamp() AND u.status='active'")
 .bind(digest(&code).as_slice()).bind(&client.issuer).bind(&client.audience).bind(redirect).bind(challenge).bind(device).bind(human.id()).bind(human.user_id()).execute(pool.inner()).await?;
 if result.rows_affected()!=1{return Err(Error::Revoked);}Ok(code)
}
/// Intentionally not Debug/Serialize: raw credentials must never be logged.
pub struct Tokens{pub session_id:Uuid,pub access:String,pub refresh:String}
pub async fn exchange(pool:&DbPool,client:&Client,code:&str,redirect:&str,verifier:&str)->Result<Tokens,Error>{
 let hash=parse(code,"ovg_mc_")?;
 if redirect!=client.redirect{return Err(Error::Invalid);}
 let mut tx=pool.inner().begin().await?;
 let row=sqlx::query("SELECT g.web_session_id,g.user_id,g.challenge,g.device_id FROM identity.mobile_grants g JOIN identity.web_sessions s ON s.id=g.web_session_id AND s.user_id=g.user_id JOIN identity.users u ON u.id=g.user_id JOIN identity.identity_links l ON l.id=s.identity_link_id AND l.user_id=u.id WHERE g.code_hash=$1 AND g.issuer=$2 AND g.audience=$3 AND g.redirect_uri=$4 AND g.used_at IS NULL AND g.expires_at>clock_timestamp() AND s.revoked_at IS NULL AND s.expires_at>clock_timestamp() AND u.status='active' FOR UPDATE OF g")
 .bind(hash.as_slice()).bind(&client.issuer).bind(&client.audience).bind(redirect).fetch_optional(&mut *tx).await?.ok_or(Error::Invalid)?;
 if !pkce_matches(verifier,row.try_get::<String,_>("challenge")?.as_str()){return Err(Error::Invalid);}
 let sid=Uuid::new_v4();let access=token("ovg_ma_")?;let refresh=token("ovg_mr_")?;
 sqlx::query("UPDATE identity.mobile_grants SET used_at=clock_timestamp() WHERE code_hash=$1").bind(hash.as_slice()).execute(&mut *tx).await?;
 sqlx::query("INSERT INTO identity.mobile_sessions(id,web_session_id,user_id,issuer,audience,device_id,access_hash,access_expires_at,idle_expires_at,absolute_expires_at) VALUES($1,$2,$3,$4,$5,$6,$7,clock_timestamp()+interval '10 minutes',clock_timestamp()+interval '30 days',clock_timestamp()+interval '90 days')")
 .bind(sid).bind(row.try_get::<Uuid,_>("web_session_id")?).bind(row.try_get::<Uuid,_>("user_id")?).bind(&client.issuer).bind(&client.audience).bind(row.try_get::<Uuid,_>("device_id")?).bind(digest(&access).as_slice()).execute(&mut *tx).await?;
 sqlx::query("INSERT INTO identity.mobile_refresh(hash,session_id) VALUES($1,$2)").bind(digest(&refresh).as_slice()).bind(sid).execute(&mut *tx).await?;
 tx.commit().await?;Ok(Tokens{session_id:sid,access,refresh})
}
pub async fn rotate(pool:&DbPool,client:&Client,raw:&str)->Result<Tokens,Error>{
 let hash=parse(raw,"ovg_mr_")?;let mut tx=pool.inner().begin().await?;
 // Lock session first; then read the refresh state under the same transaction.
 let sid:Uuid=sqlx::query_scalar("SELECT s.id FROM identity.mobile_sessions s JOIN identity.mobile_refresh r ON r.session_id=s.id WHERE r.hash=$1 AND s.issuer=$2 AND s.audience=$3 FOR UPDATE OF s")
 .bind(hash.as_slice()).bind(&client.issuer).bind(&client.audience).fetch_optional(&mut *tx).await?.ok_or(Error::Invalid)?;
 let used:bool=sqlx::query_scalar("SELECT used_at IS NOT NULL FROM identity.mobile_refresh WHERE hash=$1").bind(hash.as_slice()).fetch_one(&mut *tx).await?;
 if used{sqlx::query("UPDATE identity.mobile_sessions SET revoked_at=clock_timestamp() WHERE id=$1").bind(sid).execute(&mut *tx).await?;tx.commit().await?;return Err(Error::Revoked);}
 let active:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM identity.mobile_sessions m JOIN identity.web_sessions s ON s.id=m.web_session_id AND s.user_id=m.user_id JOIN identity.users u ON u.id=m.user_id JOIN identity.identity_links l ON l.id=s.identity_link_id AND l.user_id=u.id WHERE m.id=$1 AND m.revoked_at IS NULL AND m.idle_expires_at>clock_timestamp() AND m.absolute_expires_at>clock_timestamp() AND s.revoked_at IS NULL AND s.expires_at>clock_timestamp() AND u.status='active')").bind(sid).fetch_one(&mut *tx).await?;
 if !active{return Err(Error::Revoked);}
 let access=token("ovg_ma_")?;let refresh=token("ovg_mr_")?;
 sqlx::query("UPDATE identity.mobile_refresh SET used_at=clock_timestamp() WHERE hash=$1").bind(hash.as_slice()).execute(&mut *tx).await?;
 sqlx::query("INSERT INTO identity.mobile_refresh(hash,session_id) VALUES($1,$2)").bind(digest(&refresh).as_slice()).bind(sid).execute(&mut *tx).await?;
 sqlx::query("UPDATE identity.mobile_sessions SET access_hash=$1,access_expires_at=LEAST(absolute_expires_at,$2),idle_expires_at=LEAST(absolute_expires_at,$3) WHERE id=$4")
 .bind(digest(&access).as_slice()).bind(Utc::now()+Duration::minutes(10)).bind(Utc::now()+Duration::days(30)).bind(sid).execute(&mut *tx).await?;
 tx.commit().await?;Ok(Tokens{session_id:sid,access,refresh})
}
/// Separate human namespace. Current project policy must still be checked by
/// the route owner; the returned IDs are not a project authorization grant.
pub struct MobileActor{pub session_id:Uuid,pub user_id:Uuid,pub device_id:Uuid}
pub async fn authenticate(pool:&DbPool,client:&Client,raw:&str)->Result<MobileActor,Error>{
 let hash=parse(raw,"ovg_ma_")?;
 let row=sqlx::query("SELECT m.id,m.user_id,m.device_id FROM identity.mobile_sessions m JOIN identity.web_sessions s ON s.id=m.web_session_id AND s.user_id=m.user_id JOIN identity.users u ON u.id=m.user_id JOIN identity.identity_links l ON l.id=s.identity_link_id AND l.user_id=u.id WHERE m.access_hash=$1 AND m.issuer=$2 AND m.audience=$3 AND m.revoked_at IS NULL AND m.access_expires_at>clock_timestamp() AND m.idle_expires_at>clock_timestamp() AND m.absolute_expires_at>clock_timestamp() AND s.revoked_at IS NULL AND s.expires_at>clock_timestamp() AND u.status='active'")
 .bind(hash.as_slice()).bind(&client.issuer).bind(&client.audience).fetch_optional(pool.inner()).await?.ok_or(Error::Invalid)?;
 Ok(MobileActor{session_id:row.try_get("id")?,user_id:row.try_get("user_id")?,device_id:row.try_get("device_id")?})
}
pub async fn revoke(pool:&DbPool,actor:&MobileActor)->Result<(),Error>{
 sqlx::query("UPDATE identity.mobile_sessions SET revoked_at=clock_timestamp() WHERE id=$1 AND user_id=$2").bind(actor.session_id).bind(actor.user_id).execute(pool.inner()).await?;Ok(())
}
#[cfg(test)]
mod tests{
 use super::*;
 #[test]fn pkce_and_namespaces(){
 assert!(pkce_matches("dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk","E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM"));
 assert!(!pkce_matches("wrong","E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM"));
 let a=token("ovg_ma_").unwrap();assert!(parse(&a,"ovg_ma_").is_ok());assert!(parse(&a,"ovg_mr_").is_err());assert!(parse("ovgs_fixture","ovg_ma_").is_err());
 assert!(Client::reviewed("https://gis.example.invalid","mobile","https://mobile.example.invalid/callback").is_ok());
 assert!(Client::reviewed("https://gis.example.invalid","mobile","https://evil.example.invalid/callback?bad=1").is_err());
 }
}
