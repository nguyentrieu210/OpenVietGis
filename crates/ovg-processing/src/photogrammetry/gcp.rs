//! Bounded multi-view GCP triangulation from calibrated undistorted pixel observations.
//! Robust pair hypotheses and weighted ray intersection; not joint bundle adjustment.
use super::ortho::Camera;
use std::collections::BTreeSet;
#[derive(Debug,Clone,Copy)]
pub struct Observation{pub camera_id:u32,pub camera:Camera,pub pixel:[f64;2],pub sigma_px:f64}
#[derive(Debug,Clone,Copy)]
pub struct Policy{pub min_views:usize,pub min_angle_deg:f64,pub max_error_px:f64,pub max_hypotheses:usize}
#[derive(Debug,Clone,PartialEq)]
pub struct ResultPoint{pub xyz:[f64;3],pub inliers:Vec<u32>,pub rejected:Vec<u32>,pub rmse_px:f64}
#[derive(Debug,Clone,Copy,PartialEq,Eq)]
pub enum Error{Invalid,Duplicate,Limit,Degenerate,NoConsensus}
fn dot(a:[f64;3],b:[f64;3])->f64{(0..3).map(|i|a[i]*b[i]).sum()}
fn sub(a:[f64;3],b:[f64;3])->[f64;3]{std::array::from_fn(|i|a[i]-b[i])}
fn norm(a:[f64;3])->f64{a[0].hypot(a[1]).hypot(a[2])}
fn ray(o:&Observation)->Option<[f64;3]>{
 let c=o.camera;let v=[(o.pixel[0]-c.cx)/c.fx,(o.pixel[1]-c.cy)/c.fy,1.];
 let w:[f64;3]=std::array::from_fn(|i|(0..3).map(|j|c.world_to_camera[j][i]*v[j]).sum());
 let n=norm(w);if !n.is_finite()||n<=0.{return None;}Some(w.map(|x|x/n))
}
pub(crate) fn solve3(mut a:[[f64;3];3],mut b:[f64;3])->Option<[f64;3]>{
 let scale=a.iter().flatten().map(|x|x.abs()).fold(0.,f64::max);
 if !scale.is_finite()||scale<=0.{return None;}
 for k in 0..3{
 let p=(k..3).max_by(|&i,&j|a[i][k].abs().total_cmp(&a[j][k].abs()))?;
 if a[p][k].abs()<scale*1e-10{return None;}
 a.swap(k,p);b.swap(k,p);
 for i in k+1..3{let r=a[i][k]/a[k][k];for j in k..3{a[i][j]-=r*a[k][j];}b[i]-=r*b[k];}
 }
 let mut x=[0.;3];for i in (0..3).rev(){x[i]=(b[i]-(i+1..3).map(|j|a[i][j]*x[j]).sum::<f64>())/a[i][i];}
 x.iter().all(|v|v.is_finite()).then_some(x)
}
fn intersect(obs:&[Observation],rays:&[[f64;3]],idx:&[usize])->Option<[f64;3]>{
 let origin=obs[*idx.first()?].camera.center;
 let min_sigma=idx.iter().map(|&i|obs[i].sigma_px).fold(f64::INFINITY,f64::min);
 let(mut a,mut b)=([[0.;3];3],[0.;3]);
 for &i in idx{
 let d=rays[i];let c=sub(obs[i].camera.center,origin);let w=(min_sigma/obs[i].sigma_px).powi(2);
 for j in 0..3{for k in 0..3{
 let m=w*((if j==k{1.}else{0.})-d[j]*d[k]);a[j][k]+=m;b[j]+=m*c[k];
 }}
 }
 let x=solve3(a,b)?;Some(std::array::from_fn(|i|origin[i]+x[i]))
}
pub(crate) fn reprojection(o:&Observation,p:[f64;3])->Option<[f64;2]>{
 let c=o.camera;let d=sub(p,c.center);
 let [x,y,z]=c.world_to_camera.map(|r|dot(r,d));
 if !z.is_finite()||z<=1e-9{return None;}
 let e=[c.fx*x/z+c.cx-o.pixel[0],c.fy*y/z+c.cy-o.pixel[1]];
 e.iter().all(|v|v.is_finite()).then_some(e)
}
/// Fixed-camera weighted nonlinear reprojection refinement. Damped normal
/// equations, bounded iterations and monotone decrease of weighted residuals.
fn refine(obs:&[Observation],ids:&[usize],mut point:[f64;3])->Option<[f64;3]>{
 let min_sigma=ids.iter().map(|&i|obs[i].sigma_px).fold(f64::INFINITY,f64::min);
 let cost=|p:[f64;3]|->Option<f64>{
 let mut total=0.;
 for &i in ids{let e=reprojection(&obs[i],p)?;let w=min_sigma/obs[i].sigma_px;
 total+=(w*e[0]).powi(2)+(w*e[1]).powi(2);}
 total.is_finite().then_some(total)
 };
 let mut old=cost(point)?;let mut damping=1e-3;
 for _ in 0..24{
 if old<=1e-24{return Some(point);}
 let mut h=[[0.;3];3];let mut g=[0.;3];
 for &i in ids{
 let o=&obs[i];let c=o.camera;let [x,y,z]=c.world_to_camera.map(|r|dot(r,sub(point,c.center)));
 if z<=1e-9||!z.is_finite(){return None;}
 let e=reprojection(o,point)?;
 let w=(min_sigma/o.sigma_px).powi(2);
 let j:[[f64;3];2]=std::array::from_fn(|k|std::array::from_fn(|a|{
 let (f,v)=if k==0{(c.fx,x)}else{(c.fy,y)};
 (f/z)*(c.world_to_camera[k][a]-(v/z)*c.world_to_camera[2][a])
 }));
 for k in 0..2{for a in 0..3{
 g[a]+=w*j[k][a]*e[k];for b in 0..3{h[a][b]+=w*j[k][a]*j[k][b];}
 }}
 }
 let scale=(0..3).map(|i|h[i][i]).fold(0.,f64::max);
 if !scale.is_finite()||scale<=0.{return None;}
 for i in 0..3{h[i][i]+=damping*h[i][i].max(scale*1e-9);}
 let delta=solve3(h,g.map(|v|-v))?;
 let candidate=std::array::from_fn(|i|point[i]+delta[i]);
 if let Some(next)=cost(candidate){
 if next<old{
 let gain=old-next;point=candidate;old=next;damping=(damping*0.3).max(1e-12);
 if gain<=1e-12*old.max(1.){return Some(point);}
 continue;
 }
 }
 damping*=10.;if damping>1e12{break;}
 }
 Some(point)
}
fn score(obs:&[Observation],p:[f64;3],limit:f64)->(Vec<usize>,f64){
 let mut ids=Vec::new();let mut cost=0.;
 for(i,o)in obs.iter().enumerate(){if let Some(e)=reprojection(o,p){
 let n=e[0].hypot(e[1]);if n<=limit{ids.push(i);cost+=(n/limit).powi(2);}
 }}
 (ids,cost)
}
fn baseline(rays:&[[f64;3]],ids:&[usize],cos:f64)->bool{
 ids.iter().enumerate().any(|(i,&a)|ids[i+1..].iter().any(|&b|dot(rays[a],rays[b]).abs()<=cos))
}
pub fn triangulate(obs:&[Observation],p:Policy)->Result<ResultPoint,Error>{
 if obs.len()>64||p.max_hypotheses>2016{return Err(Error::Limit);}
 if p.min_views<2||obs.len()<p.min_views||p.max_hypotheses==0||
 !p.min_angle_deg.is_finite()||p.min_angle_deg<=0.||p.min_angle_deg>=90.||
 !p.max_error_px.is_finite()||p.max_error_px<=0.||p.max_error_px>100.{return Err(Error::Invalid);}
 let mut unique=BTreeSet::new();let mut rays=Vec::with_capacity(obs.len());
 for o in obs{
 if !unique.insert(o.camera_id){return Err(Error::Duplicate);}
 if !o.camera.valid()||!o.pixel.iter().all(|v|v.is_finite())||!o.sigma_px.is_finite()||o.sigma_px<=0.{return Err(Error::Invalid);}
 rays.push(ray(o).ok_or(Error::Invalid)?);
 }
 let cos=p.min_angle_deg.to_radians().cos();
 // Prefer well-separated rays, deterministic ties by input index.
 let mut pairs=Vec::new();
 for i in 0..obs.len(){for j in i+1..obs.len(){let angle=dot(rays[i],rays[j]).abs();if angle<=cos{pairs.push((angle,i,j));}}}
 pairs.sort_by(|a,b|a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)).then(a.2.cmp(&b.2)));
 if pairs.is_empty(){return Err(Error::Degenerate);}
 let mut best:Option<(Vec<usize>,f64)>=None;
 for (_,i,j) in pairs.into_iter().take(p.max_hypotheses){
 let Some(x)=intersect(obs,&rays,&[i,j])else{continue;};
 let (ids,cost)=score(obs,x,p.max_error_px);
 if ids.len()<p.min_views||!baseline(&rays,&ids,cos){continue;}
 if best.as_ref().is_none_or(|(b,c)|ids.len()>b.len()||(ids.len()==b.len()&&cost<*c)){best=Some((ids,cost));}
 }
 let(mut ids,_)=best.ok_or(Error::NoConsensus)?;
 // Refit consensus and re-evaluate; fail rather than publish unstable membership.
 for _ in 0..8{
 let initial=intersect(obs,&rays,&ids).ok_or(Error::Degenerate)?;
 let x=refine(obs,&ids,initial).ok_or(Error::Degenerate)?;
 let(next,cost)=score(obs,x,p.max_error_px);
 if next.len()<p.min_views||!baseline(&rays,&next,cos){return Err(Error::NoConsensus);}
 if next==ids{return Ok(ResultPoint{xyz:x,inliers:ids.iter().map(|&i|obs[i].camera_id).collect(),
 rejected:obs.iter().enumerate().filter(|(i,_)|!ids.contains(i)).map(|(_,o)|o.camera_id).collect(),
 rmse_px:p.max_error_px*(cost/ids.len() as f64).sqrt()});}
 ids=next;
 }
 Err(Error::NoConsensus)
}
#[cfg(test)]mod tests{
 use super::*;
 fn o(id:u32,x:f64,u:f64)->Observation{Observation{camera_id:id,camera:Camera{world_to_camera:[[1.,0.,0.],[0.,1.,0.],[0.,0.,1.]],center:[x,0.,0.],fx:100.,fy:100.,cx:0.,cy:0.},pixel:[u,0.],sigma_px:1.}}
 fn policy()->Policy{Policy{min_views:3,min_angle_deg:2.,max_error_px:0.5,max_hypotheses:64}}
 #[test]fn recovers_point_and_rejects_wrong_mark(){
 let a=[o(1,-2.,20.),o(2,0.,0.),o(3,2.,-20.),o(4,3.,70.)];
 let r=triangulate(&a,policy()).unwrap();assert!(norm(sub(r.xyz,[0.,0.,10.]))<1e-8);
 assert_eq!(r.inliers,vec![1,2,3]);assert_eq!(r.rejected,vec![4]);assert!(r.rmse_px<1e-8);}
 #[test]fn nonlinear_refinement_reduces_weighted_reprojection(){
 let mut a=[o(1,-2.,20.),o(2,0.,0.),o(3,2.,-20.)];a[1].pixel[1]=0.2;
 let ids=[0,1,2];let start=[0.1,0.2,9.5];let after=refine(&a,&ids,start).unwrap();
 let cost=|p|a.iter().map(|o|{let e=reprojection(o,p).unwrap();e[0]*e[0]+e[1]*e[1]}).sum::<f64>();
 assert!(cost(after)<cost(start));assert!(cost(after)<0.1);
 }
 #[test]fn duplicate_and_parallel_rays_fail(){
 assert_eq!(triangulate(&[o(1,0.,0.),o(1,1.,0.),o(3,2.,0.)],policy()),Err(Error::Duplicate));
 assert_eq!(triangulate(&[o(1,0.,0.),o(2,1.,0.),o(3,2.,0.)],policy()),Err(Error::Degenerate));}
}
