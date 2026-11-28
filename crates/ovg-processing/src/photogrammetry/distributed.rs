//! Pure dispatch admission policy. No transport, enrollment, signing or remote execution.
//! Only trusted coordinator records may be used; workers cannot self-approve.
use super::{Operation,Device,Precision,Budget};
use std::collections::BTreeSet;
#[derive(Debug,Clone)]
pub struct Worker{
 pub id:String,pub project:String,pub location:String,pub build_sha256:String,pub authenticated:bool,pub revoked:bool,
 pub qualification_expires_ms:u64,pub operations:BTreeSet<Operation>,pub device:Device,pub precision:Precision,
 pub available:Budget,pub remote:bool,
}
#[derive(Debug,Clone)]
pub struct Consent{
 pub project:String,pub job:String,pub worker:String,pub destination:String,pub input_manifest_sha256:String,
 pub allowed_bytes:u64,pub expires_ms:u64,
}
pub struct Job<'a>{
 pub project:&'a str,pub id:&'a str,pub input_manifest_sha256:&'a str,pub transfer_bytes:u64,
 pub operation:Operation,pub device:Device,pub precision:Precision,pub demand:Budget,pub offline:bool,
}
#[derive(Debug,Clone,Copy,PartialEq,Eq)]
pub enum Error{Invalid,Duplicate,NoQualifiedWorker}
/// Local workers are preferred. Exact qualified build allowlist required.
/// Actual reservation and single-use transfer accounting must be atomic at dispatch.
pub fn select<'a>(job:&Job<'_>,workers:&'a[Worker],builds:&BTreeSet<String>,consents:&[Consent],now:u64)->Result<&'a Worker,Error>{
 if workers.len()>4096||consents.len()>4096||!super::valid_id(job.project)||!super::valid_id(job.id)||
 !super::digest(job.input_manifest_sha256)||job.demand.host_bytes==0||job.demand.threads==0||
 (job.device==Device::Cpu&&job.demand.device_bytes!=0)||(job.device!=Device::Cpu&&job.demand.device_bytes==0){return Err(Error::Invalid);}
 let mut seen=BTreeSet::new();let mut best:Option<&Worker>=None;
 for w in workers{
 if !super::valid_id(&w.id)||!seen.insert(&w.id){return Err(Error::Duplicate);}
 if w.project!=job.project||!w.authenticated||w.revoked||now>=w.qualification_expires_ms||
 !super::digest(&w.build_sha256)||!builds.contains(&w.build_sha256)||!w.operations.contains(&job.operation)||
 w.device!=job.device||w.precision!=job.precision||!job.demand.fits(w.available){continue;}
 if w.remote&&(job.offline||!consents.iter().any(|c|c.project==job.project&&c.job==job.id&&c.worker==w.id&&
 c.destination==w.location&&c.input_manifest_sha256==job.input_manifest_sha256&&now<c.expires_ms&&
 job.transfer_bytes<=c.allowed_bytes)){continue;}
 let score=|w:&Worker|(w.remote,std::cmp::Reverse(w.available.host_bytes),std::cmp::Reverse(w.available.device_bytes));
 if best.is_none_or(|b|score(w)<score(b)||(score(w)==score(b)&&w.id<b.id)){best=Some(w);}
 }
 best.ok_or(Error::NoQualifiedWorker)
}
#[cfg(test)]mod tests{
 use super::*;
 fn budget()->Budget{Budget{host_bytes:100,device_bytes:0,scratch_bytes:100,threads:2}}
 fn worker(id:&str,remote:bool)->Worker{Worker{id:id.into(),project:"p".into(),location:"vn".into(),build_sha256:"a".repeat(64),
 authenticated:true,revoked:false,qualification_expires_ms:100,operations:[Operation::InspectImages].into_iter().collect(),
 device:Device::Cpu,precision:Precision::F32,available:budget(),remote}}
 fn job(d:&str)->Job<'_>{Job{project:"p",id:"j",input_manifest_sha256:d,transfer_bytes:10,operation:Operation::InspectImages,
 device:Device::Cpu,precision:Precision::F32,demand:budget(),offline:false}}
 fn builds()->BTreeSet<String>{["a".repeat(64)].into_iter().collect()}
 #[test]fn no_implicit_remote_and_local_preferred(){
 let d="b".repeat(64);let j=job(&d);let ws=[worker("remote",true),worker("local",false)];
 assert_eq!(select(&j,&ws,&builds(),&[],1).unwrap().id,"local");
 assert!(matches!(select(&j,&ws[..1],&builds(),&[],1),Err(Error::NoQualifiedWorker)));}
 #[test]fn explicit_consent_is_job_and_manifest_scoped(){
 let d="b".repeat(64);let mut j=job(&d);let ws=[worker("remote",true)];
 let mut c=Consent{project:"p".into(),job:"j".into(),worker:"remote".into(),destination:"vn".into(),input_manifest_sha256:d.clone(),allowed_bytes:10,expires_ms:20};
 assert!(select(&j,&ws,&builds(),&[c.clone()],1).is_ok());
 j.offline=true;assert!(select(&j,&ws,&builds(),&[c.clone()],1).is_err());j.offline=false;
 c.input_manifest_sha256="c".repeat(64);assert!(select(&j,&ws,&builds(),&[c],1).is_err());}
 #[test]fn revoked_worker_is_not_selected(){let d="b".repeat(64);let j=job(&d);let mut w=worker("local",false);w.revoked=true;
 assert!(matches!(select(&j,&[w],&builds(),&[],1),Err(Error::NoQualifiedWorker)));}
}
