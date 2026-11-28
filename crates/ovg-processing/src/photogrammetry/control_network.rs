//! GCP network alignment with disjoint control/check roles and robust bounded fit.
//! Coordinates must already use a qualified projected metric CRS and vertical datum.
use super::{alignment3d,colmap_model::Alignment,gcp::{self,Observation},survey::Control};
use std::collections::BTreeSet;
#[derive(Debug,Clone,Copy,PartialEq,Eq)]pub enum Role{Control,Checkpoint}
pub struct Mark<'a>{pub id:&'a str,pub role:Role,pub target:[f64;3],pub sigma_m:f64,pub observations:&'a[Observation]}
#[derive(Debug,Clone,Copy)]pub struct Policy{
 pub triangulation:gcp::Policy,pub min_controls:usize,pub min_checks:usize,
 pub max_control_residual_m:f64,pub max_xy_rmse_m:f64,pub max_z_rmse_m:f64,pub max_check_3d_m:f64,
 pub max_trials:usize,
}
#[derive(Debug)]pub struct Report{
 pub alignment:Alignment,pub fitted:Vec<String>,pub rejected_controls:Vec<String>,
 pub check:alignment3d::Check,pub passed:bool,
}
#[derive(Debug,Clone,PartialEq,Eq)]pub enum Error{Invalid,Limit,Duplicate,Triangulation(String),NoConsensus}
fn residual(a:Alignment,c:&Control)->f64{
 let x:[f64;3]=std::array::from_fn(|i|a.offset[i]+a.scale*(0..3).map(|j|a.rotation[i][j]*c.local[j]).sum::<f64>()-c.target[i]);
 x[0].hypot(x[1]).hypot(x[2])
}
fn consensus(a:Alignment,controls:&[Control],threshold:f64)->(Vec<usize>,f64){
 let mut ids=Vec::new();let mut cost=0.;
 for(i,c)in controls.iter().enumerate(){let d=residual(a,c);if d<=threshold{ids.push(i);cost+=(d/threshold).powi(2);}}
 (ids,cost)
}
fn fit_set(c:&[Control],ids:&[usize])->Option<Alignment>{
 alignment3d::fit(&ids.iter().map(|&i|c[i]).collect::<Vec<_>>()).ok()
}
/// Deterministic pseudorandom triplet proposals, bounded even for many controls.
/// Failure to find consensus is explicit, never falls back to unchecked alignment.
fn robust(c:&[Control],p:Policy)->Result<(Alignment,Vec<usize>),Error>{
 let mut best:Option<(Vec<usize>,f64)>=None;
 let mut consider=|a:Alignment|{
 let(ids,cost)=consensus(a,c,p.max_control_residual_m);
 if ids.len()>=p.min_controls&&best.as_ref().is_none_or(|(old,v)|ids.len()>old.len()||(ids.len()==old.len()&&cost<*v)){best=Some((ids,cost));}
 };
 if let Ok(a)=alignment3d::fit(c){consider(a);}
 let mut state=0xd1b54a32d192ed03u64;
 let mut next=||{state^=state<<13;state^=state>>7;state^=state<<17;(state%c.len() as u64) as usize};
 // Exhaustively enumerate small networks; for larger networks sample bounded triplets.
 let mut trials=0usize;
 if c.len()<=16{
 'outer:for i in 0..c.len(){for j in i+1..c.len(){for k in j+1..c.len(){
 if trials>=p.max_trials{break 'outer;}trials+=1;
 if let Some(a)=fit_set(c,&[i,j,k]){consider(a);}
 }}}
 }else{
 for _ in 0..p.max_trials{let ids=[next(),next(),next()];if ids[0]==ids[1]||ids[0]==ids[2]||ids[1]==ids[2]{continue;}
 if let Some(a)=fit_set(c,&ids){consider(a);}}
 }
 let(mut ids,_)=best.ok_or(Error::NoConsensus)?;
 for _ in 0..8{
 let a=fit_set(c,&ids).ok_or(Error::NoConsensus)?;
 let(next,_)=consensus(a,c,p.max_control_residual_m);
 if next.len()<p.min_controls{return Err(Error::NoConsensus);}
 if next==ids{return Ok((a,ids));}ids=next;
 }
 Err(Error::NoConsensus)
}
pub fn align(marks:&[Mark<'_>],p:Policy)->Result<Report,Error>{
 if marks.len()>512||p.max_trials>4096{return Err(Error::Limit);}
 if p.min_controls<3||p.min_checks<1||p.max_trials==0||
 ![p.max_control_residual_m,p.max_xy_rmse_m,p.max_z_rmse_m,p.max_check_3d_m].iter().all(|v|v.is_finite()&&*v>0.){return Err(Error::Invalid);}
 let mut unique=BTreeSet::new();let mut controls=Vec::new();let mut control_ids=Vec::new();let mut checks=Vec::new();
 for m in marks{
 if !super::valid_id(m.id)||!m.target.iter().all(|v|v.is_finite())||!m.sigma_m.is_finite()||m.sigma_m<=0.||m.sigma_m>1e6{return Err(Error::Invalid);}
 if !unique.insert(m.id){return Err(Error::Duplicate);}
 let point=gcp::triangulate(m.observations,p.triangulation).map_err(|_|Error::Triangulation(m.id.into()))?;
 let weight=(1./m.sigma_m).powi(2);if !weight.is_finite()||weight<=0.{return Err(Error::Invalid);}
 let c=Control{local:point.xyz,target:m.target,weight};
 match m.role{Role::Control=>{controls.push(c);control_ids.push(m.id);},Role::Checkpoint=>checks.push(c)}
 }
 if controls.len()<p.min_controls||checks.len()<p.min_checks{return Err(Error::Invalid);}
 let(a,ids)=robust(&controls,p)?;
 let check=alignment3d::check(a,&checks).map_err(|_|Error::Invalid)?;
 let passed=check.xy_rmse<=p.max_xy_rmse_m&&check.z_rmse<=p.max_z_rmse_m&&check.max_3d<=p.max_check_3d_m;
 Ok(Report{alignment:a,fitted:ids.iter().map(|&i|control_ids[i].into()).collect(),
 rejected_controls:control_ids.iter().enumerate().filter(|(i,_)|!ids.contains(i)).map(|(_,s)|(*s).into()).collect(),check,passed})
}
#[cfg(test)]mod tests{
 use super::*;use super::super::ortho::Camera;
 fn obs(x:[f64;3])->Vec<Observation>{
 [-2.,0.,2.].into_iter().enumerate().map(|(i,c)|Observation{camera_id:i as u32,
 camera:Camera{world_to_camera:[[1.,0.,0.],[0.,1.,0.],[0.,0.,1.]],center:[c,0.,0.],fx:100.,fy:100.,cx:0.,cy:0.},
 pixel:[100.*(x[0]-c)/x[2],100.*x[1]/x[2]],sigma_px:0.1}).collect()
 }
 fn p()->Policy{Policy{triangulation:gcp::Policy{min_views:3,min_angle_deg:1.,max_error_px:0.5,max_hypotheses:10},
 min_controls:4,min_checks:1,max_control_residual_m:0.05,max_xy_rmse_m:0.05,max_z_rmse_m:0.05,max_check_3d_m:0.1,max_trials:128}}
 #[test]fn robust_control_outlier_and_independent_gate(){
 let pts=[[0.,0.,10.],[2.,0.,10.],[0.,2.,10.],[2.,2.,11.],[1.,1.,12.],[3.,1.,10.]];
 let o:Vec<_>=pts.iter().copied().map(obs).collect();let names=["a","b","c","d","bad","check"];
 let mut m:Vec<_>=pts.iter().enumerate().map(|(i,x)|Mark{id:names[i],role:if i==5{Role::Checkpoint}else{Role::Control},
 target:[500000.+2.*x[0]+if i==4{20.}else{0.},1200000.+2.*x[1],100.+2.*x[2]],sigma_m:0.02,observations:&o[i]}).collect();
 let r=align(&m,p()).unwrap();assert!(r.passed);assert_eq!(r.rejected_controls,vec!["bad"]);assert_eq!(r.check.count,1);
 m[5].target[2]+=2.;let r=align(&m,p()).unwrap();assert!(!r.passed);assert_eq!(r.fitted.len(),4);
 }
 #[test]fn disallows_reusing_control_id_as_check(){
 let o=obs([0.,0.,10.]);let m=[Mark{id:"same",role:Role::Control,target:[0.;3],sigma_m:1.,observations:&o},
 Mark{id:"same",role:Role::Checkpoint,target:[0.;3],sigma_m:1.,observations:&o}];
 assert!(matches!(align(&m,p()),Err(Error::Duplicate)));}
}
