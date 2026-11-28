//! Bounded hierarchical reconstruction alignment from overlapping local models.
//! Each shared landmark ID must represent a verified common track. Aligns
//! clusters along a maximum-overlap spanning tree and checks every loop edge.
//! This is a coarse initialization, not global bundle adjustment.
use super::{alignment3d,colmap_model::Alignment,survey::Control};
use std::collections::{BTreeMap,BTreeSet};
#[derive(Debug,Clone,Copy)]pub struct Landmark{pub id:u64,pub xyz:[f64;3]}
pub struct Cluster<'a>{pub id:u32,pub landmarks:&'a[Landmark]}
#[derive(Debug,Clone,Copy)]pub struct Policy{pub min_shared:usize,pub max_pair_matches:usize,pub max_total_landmarks:usize,pub max_edge_rmse:f64,pub max_edge_error:f64}
#[derive(Debug,Clone,Copy)]pub struct EdgeReport{pub a:u32,pub b:u32,pub shared:usize,pub rmse:f64,pub max_error:f64}
#[derive(Debug)]pub struct ResultModel{pub transforms:BTreeMap<u32,Alignment>,pub edges:Vec<EdgeReport>}
#[derive(Debug,Clone,Copy,PartialEq,Eq)]pub enum Error{Invalid,Limit,Disconnected,Degenerate,Drift,Cancelled}
fn identity()->Alignment{Alignment{scale:1.,rotation:[[1.,0.,0.],[0.,1.,0.],[0.,0.,1.]],offset:[0.;3]}}
fn mul(r:[[f64;3];3],p:[f64;3])->[f64;3]{r.map(|row|(0..3).map(|i|row[i]*p[i]).sum())}
pub fn apply(t:Alignment,p:[f64;3])->[f64;3]{let q=mul(t.rotation,p);std::array::from_fn(|i|t.offset[i]+t.scale*q[i])}
fn compose(a:Alignment,b:Alignment)->Alignment{Alignment{
 scale:a.scale*b.scale,offset:apply(a,b.offset),
 rotation:std::array::from_fn(|i|std::array::from_fn(|j|(0..3).map(|k|a.rotation[i][k]*b.rotation[k][j]).sum()))}}
fn valid(a:Alignment)->bool{a.scale.is_finite()&&a.scale>0.&&a.offset.iter().chain(a.rotation.iter().flatten()).all(|v|v.is_finite())}
fn reverse(a:Alignment)->Alignment{let rotation=std::array::from_fn(|i|std::array::from_fn(|j|a.rotation[j][i]));
 let scale=1./a.scale;let offset=mul(rotation,a.offset).map(|v|-scale*v);Alignment{scale,rotation,offset}}
struct Edge{a:u32,b:u32,controls:Vec<Control>,b_to_a:Alignment}
pub fn align(clusters:&[Cluster<'_>],root:u32,p:Policy,mut cancelled:impl FnMut()->bool)->Result<ResultModel,Error>{
 if clusters.is_empty()||clusters.len()>4096||p.min_shared<3||p.max_pair_matches==0||p.max_pair_matches>5_000_000||
 p.max_total_landmarks==0||p.max_total_landmarks>2_000_000||![p.max_edge_rmse,p.max_edge_error].iter().all(|v|v.is_finite()&&*v>0.){return Err(Error::Invalid);}
 let mut ids=BTreeSet::new();let mut index=BTreeMap::<u64,Vec<(u32,[f64;3])>>::new();let mut total=0usize;
 for c in clusters{
 if cancelled(){return Err(Error::Cancelled);}
 if !ids.insert(c.id){return Err(Error::Invalid);}
 total=total.checked_add(c.landmarks.len()).ok_or(Error::Limit)?;if total>p.max_total_landmarks{return Err(Error::Limit);}
 let mut seen=BTreeSet::new();
 for l in c.landmarks{if !seen.insert(l.id)||!l.xyz.iter().all(|v|v.is_finite()){return Err(Error::Invalid);}
 let e=index.entry(l.id).or_default();if e.len()>=64{return Err(Error::Limit);}e.push((c.id,l.xyz));}
 }
 if !ids.contains(&root){return Err(Error::Invalid);}
 let mut pairs=BTreeMap::<(u32,u32),Vec<Control>>::new();let mut matches=0usize;
 for (k,views) in index.values_mut().enumerate(){
 if k%256==0&&cancelled(){return Err(Error::Cancelled);}
 views.sort_unstable_by_key(|v|v.0);
 for i in 0..views.len(){for j in i+1..views.len(){
 matches+=1;if matches>p.max_pair_matches{return Err(Error::Limit);}
 let (a,x)=views[i];let(b,y)=views[j];let list=pairs.entry((a,b)).or_default();
 if list.len()>=100_000{return Err(Error::Limit);}list.push(Control{local:y,target:x,weight:1.});
 }}
 }
 let mut edges=Vec::new();
 for((a,b),controls)in pairs{
 if cancelled(){return Err(Error::Cancelled);}
 if controls.len()<p.min_shared{continue;}
 let t=alignment3d::fit(&controls).map_err(|_|Error::Degenerate)?;
 edges.push(Edge{a,b,controls,b_to_a:t});
 }
 edges.sort_by(|a,b|b.controls.len().cmp(&a.controls.len()).then((a.a,a.b).cmp(&(b.a,b.b))));
 // Grow strongest available edge from root. Never independently translate tiles.
 let mut transforms=BTreeMap::from([(root,identity())]);
 while transforms.len()<clusters.len(){
 if cancelled(){return Err(Error::Cancelled);}
 let mut found=false;
 for e in &edges{
 match(transforms.get(&e.a).copied(),transforms.get(&e.b).copied()){
 (Some(a),None)=>{let t=compose(a,e.b_to_a);if !valid(t){return Err(Error::Drift);}transforms.insert(e.b,t);found=true;break;},
 (None,Some(b))=>{let t=compose(b,reverse(e.b_to_a));if !valid(t){return Err(Error::Drift);}transforms.insert(e.a,t);found=true;break;},
 _=>{}
 }}
 if !found{return Err(Error::Disconnected);}
 }
 let mut reports=Vec::with_capacity(edges.len());
 for e in edges{
 if cancelled(){return Err(Error::Cancelled);}
 let mut sum=0.0f64;let mut max=0.0f64;
 for c in &e.controls{
 let a=apply(transforms[&e.a],c.target);let b=apply(transforms[&e.b],c.local);
 let d=(a[0]-b[0]).hypot(a[1]-b[1]).hypot(a[2]-b[2]);
 if !d.is_finite(){return Err(Error::Drift);}sum=sum.hypot(d);max=max.max(d);
 }
 let rmse=sum/(e.controls.len() as f64).sqrt();
 if rmse>p.max_edge_rmse||max>p.max_edge_error{return Err(Error::Drift);}
 reports.push(EdgeReport{a:e.a,b:e.b,shared:e.controls.len(),rmse,max_error:max});
 }
 Ok(ResultModel{transforms,edges:reports})
}
#[cfg(test)]mod tests{
 use super::*;
 fn points(start:u64,shift:f64)->Vec<Landmark>{[[0.,0.,0.],[2.,0.,0.],[0.,2.,1.],[2.,2.,0.]].into_iter().enumerate().map(|(i,mut xyz)|{xyz[0]+=shift;Landmark{id:start+i as u64,xyz}}).collect()}
 fn policy()->Policy{Policy{min_shared:3,max_pair_matches:100,max_total_landmarks:100,max_edge_rmse:1e-6,max_edge_error:1e-5}}
 #[test]fn chain_aligns_all_clusters_in_root_frame(){
 let a=points(0,0.);let mut b=points(0,10.);b.extend(points(10,20.));let c=points(10,30.);
 let cs=[Cluster{id:1,landmarks:&a},Cluster{id:2,landmarks:&b},Cluster{id:3,landmarks:&c}];
 let q=align(&cs,1,policy(),||false).unwrap();
 assert!((q.transforms[&3].offset[0]+20.).abs()<1e-7);assert_eq!(q.edges.len(),2);
 }
 #[test]fn disconnected_and_inconsistent_overlap_fail(){
 let a=points(0,0.);let b=points(10,0.);
 assert!(matches!(align(&[Cluster{id:1,landmarks:&a},Cluster{id:2,landmarks:&b}],1,policy(),||false),Err(Error::Disconnected)));
 let mut b=points(0,10.);b[0].xyz[2]+=5.;
 assert!(matches!(align(&[Cluster{id:1,landmarks:&a},Cluster{id:2,landmarks:&b}],1,policy(),||false),Err(Error::Drift)));
 }
}
