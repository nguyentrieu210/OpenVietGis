//! Conservative incremental rebuild decision and uncertainty-aware height differences.
use super::dag::Graph;
#[derive(Debug,Clone,Copy)]
pub struct DriftEvidence{
 pub max_displacement_px:f64,pub uncertainty_px:f64,pub control_rmse_px:f64,
 pub independent_controls:u32,pub same_grid:bool,pub all_anchors_verified:bool,
}
#[derive(Debug,Clone,PartialEq,Eq)]
pub enum Decision{FullRebuild,Local{dirty:Vec<String>}}
/// Evidence must come from qualified pose projection and independent control checks.
/// Missing or unbounded evidence never permits local reuse.
pub fn plan(graph:&Graph,changed:&[String],evidence:Option<DriftEvidence>,threshold_px:f64,min_controls:u32)->Result<Decision,super::dag::Error>{
 let dirty=graph.invalidated(changed)?;
 let Some(e)=evidence else{return Ok(Decision::FullRebuild);};
 let valid=[e.max_displacement_px,e.uncertainty_px,e.control_rmse_px,threshold_px].iter().all(|v|v.is_finite()&&*v>=0.);
 if !valid||threshold_px<=0.||min_controls<3||!e.same_grid||!e.all_anchors_verified||
 e.independent_controls<min_controls||e.max_displacement_px+e.uncertainty_px>threshold_px||e.control_rmse_px>threshold_px{
 return Ok(Decision::FullRebuild);}
 Ok(Decision::Local{dirty})
}
#[derive(Debug,Clone,Copy,PartialEq)]
pub struct Difference{pub delta:f32,pub uncertainty:f32,pub significant:bool}
#[derive(Debug,Clone,Copy,PartialEq,Eq)]
pub enum Error{Invalid,Limit}
/// Independent uncertainty assumption; correlated repeat-flight errors need a different profile.
pub fn height_difference(before:&[f32],after:&[f32],sigma_before:&[f32],sigma_after:&[f32],k:f32,max_cells:usize)->Result<Vec<Option<Difference>>,Error>{
 let n=before.len();
 if n>max_cells||n>4_000_000{return Err(Error::Limit);}
 if after.len()!=n||sigma_before.len()!=n||sigma_after.len()!=n||!k.is_finite()||k<=0.{return Err(Error::Invalid);}
 let mut out=Vec::with_capacity(n);
 for i in 0..n{
 let(a,b,sa,sb)=(before[i],after[i],sigma_before[i],sigma_after[i]);
 if !a.is_finite()||!b.is_finite()||!sa.is_finite()||!sb.is_finite()||sa<0.||sb<0.{out.push(None);continue;}
 let delta=b-a;let uncertainty=sa.hypot(sb);let threshold=k*uncertainty;
 if !delta.is_finite()||!uncertainty.is_finite()||!threshold.is_finite(){out.push(None);continue;}
 out.push(Some(Difference{delta,uncertainty,significant:delta.abs()>threshold}));
 }
 Ok(out)
}
#[cfg(test)]mod tests{
 use super::*;use super::super::dag::Node;
 fn graph()->Graph{Graph::new(vec![Node{id:"camera".into(),dependencies:vec![]},Node{id:"tile".into(),dependencies:vec!["camera".into()]},Node{id:"seam".into(),dependencies:vec!["tile".into()]}],10,10).unwrap()}
 #[test]fn missing_or_excessive_drift_forces_full(){let g=graph();
 assert_eq!(plan(&g,&["camera".into()],None,1.,10).unwrap(),Decision::FullRebuild);
 let mut e=DriftEvidence{max_displacement_px:0.2,uncertainty_px:0.1,control_rmse_px:0.3,independent_controls:10,same_grid:true,all_anchors_verified:true};
 assert_eq!(plan(&g,&["camera".into()],Some(e),1.,10).unwrap(),Decision::Local{dirty:vec!["camera".into(),"tile".into(),"seam".into()]});
 e.uncertainty_px=1.;assert_eq!(plan(&g,&["camera".into()],Some(e),1.,10).unwrap(),Decision::FullRebuild);}
 #[test]fn uncertainty_and_nodata(){
 let v=height_difference(&[0.,0.,f32::NAN],&[1.,0.1,1.],&[0.1;3],&[0.1;3],2.,3).unwrap();
 assert!(v[0].unwrap().significant);assert!(!v[1].unwrap().significant);assert!(v[2].is_none());}
}
