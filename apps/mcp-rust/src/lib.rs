//! Native, transport-independent MCP session store. A caller must first
//! authenticate browser identity and OAuth subject; this store never grants
//! GIS permissions. All operations are scoped to the authenticated owner.
pub mod tools;
pub mod skills;
pub mod agent;
pub mod provider;
pub mod team;
pub mod vision;
pub mod compact;
pub mod acp;
pub mod platform;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use subtle::ConstantTimeEq;
use thiserror::Error;
use uuid::Uuid;

pub const SESSION_TTL_MS: u64 = 900_000;
pub const HEARTBEAT_TTL_MS: u64 = 15_000;
pub const MAX_SESSIONS: usize = 128;
pub const MAX_SESSIONS_PER_SUBJECT: usize = 8;
pub const MAX_PAYLOAD: usize = 85_000;

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Owner {
    pub user_id: Uuid,
    pub subject: String,
    pub organization_id: Uuid,
    pub project_id: Uuid,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all="snake_case")]
pub enum CommandState { Queued, Claimed, Done, Denied, Expired }
impl CommandState {
    fn pending(self) -> bool { matches!(self, Self::Queued | Self::Claimed) }
    fn terminal(self) -> bool { !self.pending() }
}
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Command {
    pub id: Uuid,
    pub method: String,
    pub params: Value,
    pub expires_at_ms: u64,
    pub state: CommandState,
    pub result: Option<Value>,
}
#[derive(Clone, Debug, Serialize)]
pub struct Workspace {
    pub session_id: Uuid,
    pub organization_id: Uuid,
    pub project_id: Uuid,
    pub workspace_version: u8,
    pub mode: String,
    pub expires_in: u64,
}
#[derive(Serialize)]
pub struct Opened {
    pub session_id: Uuid,
    pub browser_token: String,
    pub expires_in: u64,
}
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Receipt {
    pub state: CommandState,
    #[serde(skip_serializing_if="Option::is_none")]
    pub result: Option<Value>,
}
struct Session {
    owner: Owner,
    token_hash: [u8; 32],
    version: u8,
    mode: String,
    deadline: u64,
    last_seen: u64,
    last_submit: Option<u64>,
    command: Option<Command>,
    receipt: Option<(Uuid, Receipt)>,
}
#[derive(Debug, Error, Eq, PartialEq)]
pub enum Error {
    #[error("Workspace not connected or no longer authorized")]
    Unavailable,
    #[error("Several workspaces are connected; select a session_id explicitly")]
    AmbiguousWorkspace,
    #[error("Previous command still pending or unread")]
    PreviousUnread,
    #[error("Try again after two seconds")]
    RateLimited,
    #[error("Workspace capacity reached")]
    Capacity,
    #[error("Invalid or oversized command/result")]
    Invalid,
    #[error("Command unavailable")]
    CommandUnavailable,
    #[error("OAuth token expired")]
    TokenExpired,
    #[error("Update the workspace to use this tool")]
    Version,
    #[error("Tool is not available in the active workspace mode")]
    ModeMismatch,
}
fn hash(value: &str) -> [u8; 32] { Sha256::digest(value.as_bytes()).into() }
fn secret() -> String { format!("{}{}", Uuid::new_v4().simple(), Uuid::new_v4().simple()) }
fn bounded(value: &Value) -> bool {
    if serde_json::to_vec(value).map_or(true, |v| v.len() > MAX_PAYLOAD) { return false; }
    let mut stack = vec![(value, 0)];
    while let Some((v,d)) = stack.pop() {
        if d>64 { return false; }
        match v {
            Value::Array(a)=>stack.extend(a.iter().map(|x|(x,d+1))),
            Value::Object(o)=>stack.extend(o.values().map(|x|(x,d+1))),
            _=>{}
        }
    }
    true
}
#[derive(Default)]
pub struct Store { sessions: HashMap<Uuid, Session> }
impl Store {
    fn purge(&mut self, now: u64) {
        self.sessions.retain(|_,s| now < s.deadline && now.saturating_sub(s.last_seen) <= HEARTBEAT_TTL_MS);
    }
    fn expire(s: &mut Session, now: u64) {
        if let Some(c)=s.command.as_mut() {
            if c.state.pending() && now>=c.expires_at_ms { c.state=CommandState::Expired; }
        }
    }
    pub fn open(&mut self, owner: Owner, version: u8, now: u64) -> Result<Opened, Error> {
        self.purge(now);
        if owner.subject.is_empty() || owner.subject.len()>512 || version>2 { return Err(Error::Invalid); }
        if self.sessions.len()>=MAX_SESSIONS || self.sessions.values().filter(|s|s.owner.subject==owner.subject).count()>=MAX_SESSIONS_PER_SUBJECT {
            return Err(Error::Capacity);
        }
        let session_id=Uuid::new_v4();
        let browser_token=secret();
        self.sessions.insert(session_id, Session { owner, token_hash:hash(&browser_token),version,mode:"gis".into(),deadline:now.saturating_add(SESSION_TTL_MS),last_seen:now,last_submit:None,command:None,receipt:None });
        Ok(Opened{session_id,browser_token,expires_in:SESSION_TTL_MS/1000})
    }
    /// Never return other users' workspaces, even if they share a project.
    pub fn list(&mut self, subject:&str, now:u64)->Vec<Workspace> {
        self.purge(now);
        let mut rows:Vec<_>=self.sessions.iter().filter(|(_,s)|s.owner.subject==subject).map(|(id,s)|Workspace{session_id:*id,organization_id:s.owner.organization_id,project_id:s.owner.project_id,workspace_version:s.version,mode:s.mode.clone(),expires_in:s.deadline.saturating_sub(now)/1000}).collect();
        rows.sort_by_key(|r|r.session_id);
        rows
    }
    /// A single session is backward compatible; several sessions require an
    /// explicit session ID. No "last active" heuristics for destructive writes.
    pub fn resolve(&mut self, subject:&str, target:Option<Uuid>, now:u64)->Result<Uuid,Error> {
        self.purge(now);
        if let Some(id)=target {
            return self.sessions.get(&id).filter(|s|s.owner.subject==subject).map(|_|id).ok_or(Error::Unavailable);
        }
        let mut ids=self.sessions.iter().filter(|(_,s)|s.owner.subject==subject).map(|(id,_)|*id);
        let id=ids.next().ok_or(Error::Unavailable)?;
        if ids.next().is_some(){return Err(Error::AmbiguousWorkspace);}
        Ok(id)
    }
    pub fn browser_owner(&mut self,id:Uuid,token:&str,now:u64)->Result<Owner,Error> {
        self.purge(now);
        if token.len()!=64 {return Err(Error::Unavailable);}
        let s=self.sessions.get(&id).ok_or(Error::Unavailable)?;
        if !bool::from(s.token_hash.ct_eq(&hash(token))){return Err(Error::Unavailable);}
        Ok(s.owner.clone())
    }
    fn browser(&mut self,id:Uuid,token:&str,owner:&Owner,now:u64)->Result<&mut Session,Error> {
        if self.browser_owner(id,token,now)?!=*owner {return Err(Error::Unavailable);}
        self.sessions.get_mut(&id).ok_or(Error::Unavailable)
    }
    pub fn set_mode(&mut self,id:Uuid,token:&str,owner:&Owner,mode:&str,now:u64)->Result<(),Error>{
        if !["gis","cad","3d"].contains(&mode){return Err(Error::Invalid)}
        let s=self.browser(id,token,owner,now)?;Self::expire(s,now);
        if s.mode!=mode {
            if let Some(c)=s.command.as_mut().filter(|c|c.state.pending()){c.state=CommandState::Denied;c.result=None;}
            s.mode=mode.into();
        }
        Ok(())
    }
    fn mode_allows(mode:&str,method:&str)->bool{if method.starts_with("cad_"){mode=="cad"}else if method.starts_with("map_"){mode=="gis"}else{mode!="cad"}}
    pub fn ping(&mut self,id:Uuid,token:&str,owner:&Owner,now:u64)->Result<(),Error> {
        let s=self.browser(id,token,owner,now)?;s.last_seen=now;Self::expire(s,now);Ok(())
    }
    pub fn poll(&mut self,id:Uuid,token:&str,owner:&Owner,now:u64)->Result<Option<Command>,Error> {
        let s=self.browser(id,token,owner,now)?;s.last_seen=now;Self::expire(s,now);
        Ok(s.command.as_mut().filter(|c|c.state==CommandState::Queued).map(|c|{c.state=CommandState::Claimed;c.clone()}))
    }
    pub fn prepare(&mut self,id:Uuid,token:&str,owner:&Owner,cid:Uuid,now:u64)->Result<u64,Error> {
        let s=self.browser(id,token,owner,now)?;Self::expire(s,now);
        s.command.as_ref().filter(|c|c.id==cid&&c.state==CommandState::Claimed&&Self::mode_allows(&s.mode,&c.method)).map(|c|c.expires_at_ms.saturating_sub(now)).ok_or(Error::CommandUnavailable)
    }
    /// Payload and method must have been validated against the pinned tool
    /// contract by the transport before submission. This still enforces the
    /// session version, OAuth expiry and resource bounds independently.
    pub fn submit(&mut self,subject:&str,target:Option<Uuid>,method:&str,params:Value,min_version:u8,ttl_ms:u64,token_deadline_ms:u64,now:u64)->Result<(Uuid,Uuid),Error> {
        if now>=token_deadline_ms{return Err(Error::TokenExpired);}
        if !params.is_object()||!bounded(&params)||method.is_empty()||method.len()>128||ttl_ms==0||ttl_ms>840_000{return Err(Error::Invalid);}
        let id=self.resolve(subject,target,now)?;
        let s=self.sessions.get_mut(&id).ok_or(Error::Unavailable)?;
        if s.version<min_version{return Err(Error::Version);}
        if !Self::mode_allows(&s.mode,method){return Err(Error::ModeMismatch);}
        Self::expire(s,now);
        if s.command.as_ref().is_some_and(|c|c.state!=CommandState::Expired){return Err(Error::PreviousUnread);}
        if s.last_submit.is_some_and(|v|now.saturating_sub(v)<2000){return Err(Error::RateLimited);}
        let cid=Uuid::new_v4();s.last_submit=Some(now);
        s.command=Some(Command{id:cid,method:method.into(),params,expires_at_ms:s.deadline.min(token_deadline_ms).min(now.saturating_add(ttl_ms)),state:CommandState::Queued,result:None});
        Ok((id,cid))
    }
    /// An acknowledgement is not accepted until the command was claimed,
    /// identity revalidated, and the exact operation remains unexpired.
    pub fn ack(&mut self,id:Uuid,token:&str,owner:&Owner,cid:Uuid,ok:bool,result:Value,now:u64)->Result<(),Error> {
        let s=self.browser(id,token,owner,now)?;s.last_seen=now;Self::expire(s,now);
        let c=s.command.as_mut().filter(|c|c.id==cid&&c.state==CommandState::Claimed).ok_or(Error::CommandUnavailable)?;
        c.result=if ok{Some(tools::clean_result(&c.method,&result)?)}else{None};c.state=if ok{CommandState::Done}else{CommandState::Denied};Ok(())
    }
    /// Receipt retries return the terminal result only to the original OAuth
    /// subject and workspace; a command ticket can never target another tab.
    pub fn result(&mut self,subject:&str,id:Uuid,cid:Uuid,now:u64)->Result<Receipt,Error> {
        self.resolve(subject,Some(id),now)?;
        let s=self.sessions.get_mut(&id).ok_or(Error::Unavailable)?;Self::expire(s,now);
        if let Some((ticket,r))=&s.receipt {if *ticket==cid{return Ok(r.clone());}}
        let c=s.command.as_ref().filter(|c|c.id==cid).ok_or(Error::CommandUnavailable)?;
        let r=Receipt{state:c.state,result:c.result.clone()};
        if c.state.terminal(){s.command=None;s.receipt=Some((cid,r.clone()));}Ok(r)
    }
    pub fn cancel(&mut self,subject:&str,id:Uuid,cid:Uuid,now:u64)->Result<(),Error>{
        self.resolve(subject,Some(id),now)?;
        let s=self.sessions.get_mut(&id).ok_or(Error::Unavailable)?;
        if s.command.as_ref().is_some_and(|c|c.id==cid&&c.state.pending()){s.command=None;}Ok(())
    }
    pub fn close(&mut self,id:Uuid,token:&str,owner:&Owner,now:u64)->Result<(),Error>{
        self.browser(id,token,owner,now)?;self.sessions.remove(&id);Ok(())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    fn owner(subject:&str)->Owner{Owner{user_id:Uuid::new_v4(),subject:subject.into(),organization_id:Uuid::new_v4(),project_id:Uuid::new_v4()}}
    #[test] fn mode_change_invalidates_pending_ticket(){
        let mut s=Store::default();let a=owner("alice");
        let x=s.open(a.clone(),2,0).unwrap();
        let (_,cid)=s.submit("alice",Some(x.session_id),"map_view",json!({}),0,40_000,90_000,0).unwrap();
        assert_eq!(s.poll(x.session_id,&x.browser_token,&a,1).unwrap().unwrap().id,cid);
        s.set_mode(x.session_id,&x.browser_token,&a,"cad",2).unwrap();
        s.set_mode(x.session_id,&x.browser_token,&a,"gis",3).unwrap();
        assert_eq!(s.prepare(x.session_id,&x.browser_token,&a,cid,4),Err(Error::CommandUnavailable));
        assert_eq!(s.result("alice",x.session_id,cid,4).unwrap().state,CommandState::Denied);
    }
    #[test] fn multi_project_explicit_target_and_isolation(){
        let mut s=Store::default();let a=owner("alice");let mut b=a.clone();b.project_id=Uuid::new_v4();
        let sa=s.open(a.clone(),2,0).unwrap();let sb=s.open(b.clone(),2,0).unwrap();let mallory=s.open(owner("mallory"),2,0).unwrap();
        assert_eq!(s.list("alice",0).len(),2);assert_eq!(s.resolve("alice",None,0),Err(Error::AmbiguousWorkspace));assert_eq!(s.resolve("alice",Some(mallory.session_id),0),Err(Error::Unavailable));
        let (sid,cid)=s.submit("alice",Some(sb.session_id),"map_view",json!({}),0,40_000,90_000,0).unwrap();assert_eq!(sid,sb.session_id);
        assert!(s.poll(sa.session_id,&sa.browser_token,&a,1).unwrap().is_none());assert_eq!(s.poll(sb.session_id,&sa.browser_token,&b,1),Err(Error::Unavailable));
        assert_eq!(s.poll(sb.session_id,&sb.browser_token,&b,1).unwrap().unwrap().id,cid);
    }
    #[test] fn consent_lifecycle_expiry_and_receipt_retry(){
        let mut s=Store::default();let a=owner("a");let x=s.open(a.clone(),2,0).unwrap();
        let (id,c)=s.submit("a",None,"map_view",json!({}),0,40_000,10_000,0).unwrap();
        assert_eq!(s.ack(id,&x.browser_token,&a,c,true,json!({}),1),Err(Error::CommandUnavailable));
        s.poll(id,&x.browser_token,&a,1).unwrap();assert_eq!(s.prepare(id,&x.browser_token,&a,c,2).unwrap(),9998);
        s.ack(id,&x.browser_token,&a,c,true,json!({"crs":"EPSG:4326","center":[0,0],"bbox":[-1,-1,1,1],"zoom":1}),3).unwrap();let r=s.result("a",id,c,4).unwrap();assert_eq!(r.state,CommandState::Done);assert_eq!(s.result("a",id,c,5).unwrap(),r);
        assert_eq!(s.submit("a",None,"map_view",json!({}),0,40_000,10_000,1000),Err(Error::RateLimited));
        let (_,c)=s.submit("a",None,"map_view",json!({}),0,40_000,10_000,2000).unwrap();
        s.poll(id,&x.browser_token,&a,2001).unwrap();assert_eq!(s.prepare(id,&x.browser_token,&a,c,10_000),Err(Error::CommandUnavailable));assert_eq!(s.result("a",id,c,10_001).unwrap().state,CommandState::Expired);
    }
    #[test] fn explicit_cancel_removes_pending_command(){
        let mut s=Store::default();let a=owner("a");let x=s.open(a.clone(),2,0).unwrap();
        let (id,c)=s.submit("a",Some(x.session_id),"map_view",json!({}),0,40_000,10_000,0).unwrap();
        assert_eq!(s.cancel("a",id,c,1),Ok(()));assert_eq!(s.result("a",id,c,2),Err(Error::CommandUnavailable));
    }
    #[test] fn wrong_owner_heartbeat_and_version(){
        let mut s=Store::default();let a=owner("a");let x=s.open(a.clone(),0,0).unwrap();let mut wrong=a.clone();wrong.project_id=Uuid::new_v4();
        assert_eq!(s.ping(x.session_id,&x.browser_token,&wrong,1),Err(Error::Unavailable));assert_eq!(s.submit("a",None,"gis_layers",json!({}),1,40_000,90_000,1),Err(Error::Version));assert_eq!(s.resolve("a",None,15_001),Err(Error::Unavailable));
    }
}
