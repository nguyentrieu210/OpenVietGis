//! Match externally processed GNSS exposure positions to calibrated cameras.
//! Input times must already share one monotonic GNSS timescale; positions are
//! camera centers in the same qualified metric frame as bundle adjustment.
//! No RINEX, ambiguity resolution, geoid or lever-arm estimation here.
use super::bundle::Prior;
use std::collections::BTreeSet;
#[derive(Debug,Clone,Copy,PartialEq,Eq)]
pub enum Quality{Fixed,Float,Standalone,Invalid}
#[derive(Debug,Clone,Copy)]
pub struct Fix{pub time_ns:i64,pub center:[f64;3],pub sigma_m:[f64;3],pub quality:Quality}
#[derive(Debug,Clone,Copy)]
pub struct Exposure{pub camera_id:u32,pub time_ns:i64}
#[derive(Debug,Clone,Copy)]
pub struct Policy{pub max_delta_ns:u64,pub max_speed_mps:f64,pub min_sigma_m:f64,pub max_sigma_m:f64,pub allow_float:bool}
#[derive(Debug,Clone,Copy,PartialEq,Eq)]
pub enum Error{Invalid,Limit,Duplicate}
#[derive(Debug,Clone,Copy,PartialEq,Eq)]
pub enum Status{Matched,NoNearbyFix,Ambiguous,RejectedQuality,FixAlreadyUsed,UncertaintyExceeded}
#[derive(Debug,Clone,Copy)]
pub struct Match{pub camera_id:u32,pub status:Status,pub prior:Option<Prior>,pub delta_ns:Option<u64>}
fn rejected(id:u32,status:Status,delta_ns:Option<u64>)->Match{Match{camera_id:id,status,prior:None,delta_ns}}
/// Nearest exposure-event fix; no interpolation and no implicit time offset.
/// Each event may be assigned once. A speed bound contributes a conservative
/// additive position uncertainty for timestamp mismatch.
pub fn associate(exposures:&[Exposure],fixes:&[Fix],p:Policy)->Result<Vec<Match>,Error>{
 if exposures.len()>100_000||fixes.len()>1_000_000{return Err(Error::Limit);}
 if p.max_delta_ns==0||p.max_delta_ns>1_000_000_000||!p.max_speed_mps.is_finite()||p.max_speed_mps<0.||
 !p.min_sigma_m.is_finite()||p.min_sigma_m<1e-6||!p.max_sigma_m.is_finite()||p.max_sigma_m<p.min_sigma_m{return Err(Error::Invalid);}
 if fixes.windows(2).any(|w|w[0].time_ns>=w[1].time_ns)||
 fixes.iter().any(|f|!f.center.iter().all(|v|v.is_finite())||!f.sigma_m.iter().all(|v|v.is_finite()&&*v>0.)){return Err(Error::Invalid);}
 let mut ids=BTreeSet::new();let mut times=BTreeSet::new();
 let mut proposals=Vec::with_capacity(exposures.len());
 for(i,e)in exposures.iter().enumerate(){
 if !ids.insert(e.camera_id)||!times.insert(e.time_ns){return Err(Error::Duplicate);}
 let j=fixes.partition_point(|f|f.time_ns<e.time_ns);
 let left=j.checked_sub(1).map(|k|(k,fixes[k].time_ns.abs_diff(e.time_ns)));
 let right=fixes.get(j).map(|f|(j,f.time_ns.abs_diff(e.time_ns)));
 let candidate=match(left,right){
 (Some(a),Some(b))if a.1==b.1&&a.1<=p.max_delta_ns=>{proposals.push((i,None,rejected(e.camera_id,Status::Ambiguous,Some(a.1))));continue;},
 (Some(a),Some(b))=>Some(if a.1<b.1{a}else{b}),(a,b)=>a.or(b)};
 let Some((k,dt))=candidate else{proposals.push((i,None,rejected(e.camera_id,Status::NoNearbyFix,None)));continue;};
 if dt>p.max_delta_ns{proposals.push((i,None,rejected(e.camera_id,Status::NoNearbyFix,Some(dt))));continue;}
 let f=fixes[k];
 if f.quality!=Quality::Fixed&&!(p.allow_float&&f.quality==Quality::Float){
 proposals.push((i,None,rejected(e.camera_id,Status::RejectedQuality,Some(dt))));continue;}
 let timing=p.max_speed_mps*(dt as f64/1e9);
 let sigma=f.sigma_m.map(|v|v.max(p.min_sigma_m)+timing);
 if sigma.iter().any(|v|!v.is_finite()||*v>p.max_sigma_m){
 proposals.push((i,None,rejected(e.camera_id,Status::UncertaintyExceeded,Some(dt))));continue;}
 proposals.push((i,Some(k),Match{camera_id:e.camera_id,status:Status::Matched,prior:Some(Prior{center:f.center,sigma}),delta_ns:Some(dt)}));
 }
 // Closest event wins; a tie between two exposures invalidates both.
 let mut owners=std::collections::BTreeMap::<usize,(u64,usize,bool)>::new();
 for(i,k,m)in &proposals{if let Some(k)=k{
 let dt=m.delta_ns.unwrap_or(u64::MAX);
 owners.entry(*k).and_modify(|v|{if dt<v.0{*v=(dt,*i,false);}else if dt==v.0{v.2=true;}}).or_insert((dt,*i,false));
 }}
 Ok(proposals.into_iter().map(|(i,k,m)|{
 if let Some(k)=k{let (_,owner,tied)=owners[&k];if tied||owner!=i{return rejected(m.camera_id,if tied{Status::Ambiguous}else{Status::FixAlreadyUsed},m.delta_ns);}}
 m
 }).collect())
}
#[cfg(test)]mod tests{
 use super::*;
 fn policy()->Policy{Policy{max_delta_ns:100_000_000,max_speed_mps:10.,min_sigma_m:0.02,max_sigma_m:0.5,allow_float:false}}
 fn fix(t:i64,q:Quality)->Fix{Fix{time_ns:t,center:[1.,2.,3.],sigma_m:[0.01,0.02,0.04],quality:q}}
 #[test]fn matches_fixed_and_accounts_timing_uncertainty(){
 let r=associate(&[Exposure{camera_id:1,time_ns:1_001_000_000}],&[fix(1_000_000_000,Quality::Fixed)],policy()).unwrap();
 assert_eq!(r[0].status,Status::Matched);let a=r[0].prior.unwrap();assert!((a.sigma[0]-0.03).abs()<1e-10);assert_eq!(a.center,[1.,2.,3.]);}
 #[test]fn rejects_float_and_ambiguous_events(){
 let r=associate(&[Exposure{camera_id:1,time_ns:100}],&[fix(100,Quality::Float)],policy()).unwrap();assert_eq!(r[0].status,Status::RejectedQuality);
 let r=associate(&[Exposure{camera_id:1,time_ns:100}],&[fix(90,Quality::Fixed),fix(110,Quality::Fixed)],policy()).unwrap();assert_eq!(r[0].status,Status::Ambiguous);}
 #[test]fn does_not_assign_fix_twice(){
 let r=associate(&[Exposure{camera_id:1,time_ns:100},Exposure{camera_id:2,time_ns:101}],&[fix(100,Quality::Fixed)],policy()).unwrap();
 assert_eq!(r[0].status,Status::Matched);assert_eq!(r[1].status,Status::FixAlreadyUsed);
 let r=associate(&[Exposure{camera_id:1,time_ns:99},Exposure{camera_id:2,time_ns:101}],&[fix(100,Quality::Fixed)],policy()).unwrap();
 assert!(r.iter().all(|m|m.status==Status::Ambiguous));}
}
