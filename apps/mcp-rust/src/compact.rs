//! Context compaction adapted from RustGIS assistant/compact.rs (Apache-2.0).
//! Upstream: https://github.com/opengeos/RustGIS/blob/aea84131974b57ff6359666dbe79f88054a64bf1/crates/assistant/src/compact.rs
//! Keep calls with results, trim only completed groups, and distinguish generated
//! context from user intent. OpenVietGIS uses a deterministic receipt checkpoint
//! instead of trusting a second model to summarize authorization or operation state.
use crate::agent::Turn;
use serde_json::{json,Value,Map};
use sha2::{Digest,Sha256};
pub const BUDGET:usize=400_000;
pub const TARGET:usize=200_000;
pub const OVERFLOW_TARGET:usize=48_000;
pub const HISTORY_LIMIT:usize=8_000_000;
const TRIMMED_RESULT:usize=1500;
#[derive(Clone,Debug,Default,serde::Serialize)]
pub struct Stats{pub original_bytes:usize,pub sent_bytes:usize,pub compacted_groups:usize,pub compacted_calls:usize}
pub fn size(turns:&[Turn])->usize{
    struct Counter(usize);
    impl std::io::Write for Counter{
        fn write(&mut self,b:&[u8])->std::io::Result<usize>{self.0=self.0.saturating_add(b.len());Ok(b.len())}
        fn flush(&mut self)->std::io::Result<()>{Ok(())}
    }
    let mut c=Counter(0);if serde_json::to_writer(&mut c,turns).is_err(){return usize::MAX}c.0
}
fn clip(s:&str,n:usize)->String{
    if s.len()<=n{return s.to_owned()}
    let mut end=n;while !s.is_char_boundary(end){end-=1}
    format!("{} … [trimmed]",&s[..end])
}
fn digest(v:&Value)->String{format!("{:x}",Sha256::digest(v.to_string().as_bytes()))}
// Extract bounded exact scalar receipt metadata, never interpret dataset content.
fn facts(v:&Value)->Value{
    const KEYS:&[&str]=&["id","layer_id","source_id","project_id","organization_id","feature_id","operation_id","operation_ticket","job_id","changeset_id","layout_id","revision","content_revision","schema_revision","configuration_revision","crs","state","status","acknowledged","count","total","next_after_id","ok","complete"];
    let mut out=Map::new();
    for (prefix,obj) in [("",v),("result.",&v["result"]),("receipt.",&v["receipt"])]{
        if let Some(obj)=obj.as_object(){for k in KEYS{
            if let Some(x)=obj.get(*k){
                if (x.is_string()&&x.as_str().is_some_and(|s|s.len()<=128))||x.is_number()||x.is_boolean()||x.is_null(){out.insert(format!("{prefix}{k}"),x.clone());}
            }
        }}
    }
    Value::Object(out)
}
fn groups(turns:&[Turn])->Option<Vec<(usize,usize)>>{
    if !matches!(turns.first(),Some(Turn::User{..})){return None}
    let mut pos=1;let mut groups=Vec::new();
    while pos<turns.len(){
        let Turn::Assistant{calls,..}=&turns[pos]else{return None};
        if calls.is_empty(){return None}
        let end=pos.checked_add(1+calls.len())?;
        if end>turns.len(){return None}
        for (call,t) in calls.iter().zip(&turns[pos+1..end]){
            if !matches!(t,Turn::Tool{call_id,error:false,..} if call_id==&call.id){return None}
        }
        groups.push((pos,end));pos=end;
    }
    Some(groups)
}
/// Build a smaller model projection. The original ledger is never mutated.
/// Unfinished or failed tool groups are not compacted. Keep latest full groups
/// where possible and keep all completed call IDs, hashes and exact scalar facts.
pub fn project(turns:&[Turn],force:bool)->Option<(Vec<Turn>,Stats)>{
    let original_bytes=size(turns);
    let mut stats=Stats{original_bytes,sent_bytes:original_bytes,..Stats::default()};
    if !force&&original_bytes<=BUDGET{return Some((turns.to_vec(),stats))}
    let groups=groups(turns)?;
    let target=if force{OVERFLOW_TARGET}else{TARGET};
    let mut entries=Vec::new();
    for (i,(start,end)) in groups.iter().copied().enumerate(){
        let Turn::Assistant{calls,..}=&turns[start]else{return None};
        for (c,t) in calls.iter().zip(&turns[start+1..end]){
            let Turn::Tool{observation,..}=t else{return None};
            entries.push(json!({"call_id":c.id,"tool":c.name,"arguments_sha256":digest(&c.arguments),"argument_facts":facts(&c.arguments),"result_sha256":digest(observation),"receipt_facts":facts(observation),"result_excerpt":clip(&observation.to_string(),TRIMMED_RESULT),"complete":true,"excerpt_may_be_incomplete":true}));
        }
        let mut projected=vec![turns[0].clone(),Turn::Checkpoint{entries:entries.clone()}];
        projected.extend_from_slice(&turns[end..]);
        let sent=size(&projected);
        if sent<=target||i+1==groups.len(){
            stats.sent_bytes=sent;stats.compacted_groups=i+1;stats.compacted_calls=entries.len();
            // A projection which did not shrink cannot fix a provider overflow.
            if sent<original_bytes&&sent<=BUDGET{return Some((projected,stats))}
            return None
        }
    }
    None
}
/// Only recognized context-length failures, not ordinary validation or billing errors.
pub fn overflowed(status:u16,v:&Value)->bool{
    if !matches!(status,400|413|422){return false}
    let code=v.pointer("/error/code").and_then(Value::as_str).unwrap_or("");
    let message=v.pointer("/error/message").and_then(Value::as_str).unwrap_or("").to_ascii_lowercase();
    code=="context_length_exceeded"||["context length","context window","prompt is too long","input is too long","too many input tokens","input token count","exceeds the available context size"].iter().any(|p|message.contains(p))
}
#[cfg(test)]mod tests{
    use super::*;use crate::agent::ToolCall;
    fn history()->Vec<Turn>{
        let mut v=vec![Turn::User{text:"Inspect, do not edit".into()}];
        for i in 0..5{
            let id=format!("call-{i}");v.push(Turn::Assistant{text:"".into(),calls:vec![ToolCall{id:id.clone(),name:"gis_query".into(),arguments:json!({"layer_id":"exact-layer","content_revision":7})}]});
            v.push(Turn::Tool{call_id:id,observation:json!({"acknowledged":true,"result":{"layer_id":"exact-layer","count":17,"data":"é".repeat(55_000)}}),error:false});
        }v
    }
    #[test]fn shrinks_completed_groups_without_changing_ledger(){
        let v=history();let before=serde_json::to_value(&v).unwrap();let (out,s)=project(&v,false).unwrap();
        assert!(s.sent_bytes<=TARGET&&s.compacted_groups>0);assert_eq!(serde_json::to_value(&v).unwrap(),before);
        assert!(matches!(&out[0],Turn::User{text} if text=="Inspect, do not edit"));
        let Turn::Checkpoint{entries}=&out[1]else{panic!("missing checkpoint")};
        assert_eq!(entries[0]["receipt_facts"]["result.count"],17);assert_eq!(entries[0]["argument_facts"]["content_revision"],7);
        assert_eq!(entries[0]["call_id"],"call-0");
    }
    #[test]fn refuses_unpaired_or_failed_group(){
        let mut v=history();v.pop();assert!(project(&v,true).is_none());
        let mut v=history();if let Some(Turn::Tool{error,..})=v.last_mut(){*error=true}assert!(project(&v,true).is_none());
    }
    #[test]fn small_history_unchanged_and_overflow_classified(){
        let v=vec![Turn::User{text:"hello".into()}];assert_eq!(project(&v,false).unwrap().1.compacted_groups,0);
        assert!(overflowed(400,&json!({"error":{"code":"context_length_exceeded"}})));
        assert!(!overflowed(429,&json!({"error":{"message":"context window"}})));
        assert!(!overflowed(400,&json!({"error":{"code":"invalid_api_key"}})));
        assert!(clip("ééé",3).starts_with("é "));
    }
}
