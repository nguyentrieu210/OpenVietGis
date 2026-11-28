//! Bounded reference block-coordinate bundle adjustment for calibrated pinhole cameras.
//! Alternates point and six-DOF pose updates, with robust reprojection loss.
//! Two distinct fixed camera centers anchor gauge; not a sparse Schur solver.
use super::ortho::Camera;
use std::collections::{BTreeSet,VecDeque};
#[derive(Debug,Clone,Copy)]
pub struct Prior{pub center:[f64;3],pub sigma:[f64;3]}
#[derive(Debug,Clone,Copy)]
pub struct Pose{pub camera:Camera,pub fixed:bool,pub prior:Option<Prior>}
#[derive(Debug,Clone,Copy)]
pub struct Point{pub xyz:[f64;3],pub fixed:bool}
#[derive(Debug,Clone,Copy)]
pub struct Mark{pub camera:usize,pub point:usize,pub pixel:[f64;2],pub sigma:f64}
#[derive(Debug,Clone,Copy)]
pub struct Policy{pub iterations:u32,pub huber:f64,pub min_gain:f64}
#[derive(Debug,Clone,Copy)]
pub struct Report{pub initial_cost:f64,pub final_cost:f64,pub sweeps:u32,pub accepted:u64}
#[derive(Debug,Clone,Copy,PartialEq,Eq)]
pub enum Error{Invalid,Limit,Unanchored,Cancelled}
fn dot(a:[f64;3],b:[f64;3])->f64{(0..3).map(|i|a[i]*b[i]).sum()}
fn norm(a:[f64;3])->f64{a[0].hypot(a[1]).hypot(a[2])}
fn sub(a:[f64;3],b:[f64;3])->[f64;3]{std::array::from_fn(|i|a[i]-b[i])}
fn projection(c:Camera,p:[f64;3],m:&Mark)->Option<([f64;2],[[f64;3];2],[f64;3])>{
 let v=c.world_to_camera.map(|r|dot(r,sub(p,c.center)));
 let [x,y,z]=v;if !z.is_finite()||z<=1e-9{return None;}
 let e=[(c.fx*x/z+c.cx-m.pixel[0])/m.sigma,(c.fy*y/z+c.cy-m.pixel[1])/m.sigma];
 let j=[[c.fx/z/m.sigma,0.,-c.fx*(x/z)/z/m.sigma],[0.,c.fy/z/m.sigma,-c.fy*(y/z)/z/m.sigma]];
 if !e.iter().chain(j.iter().flatten()).all(|v|v.is_finite()){return None;}Some((e,j,v))
}
fn loss(e:[f64;2],h:f64)->f64{let n=e[0].hypot(e[1]);if n<=h{0.5*n*n}else{h*(n-0.5*h)}}
fn weight(e:[f64;2],h:f64)->f64{let n=e[0].hypot(e[1]);if n<=h{1.}else{h/n}}
fn prior_cost(p:Pose)->f64{p.prior.map_or(0.,|a|0.5*(0..3).map(|i|((p.camera.center[i]-a.center[i])/a.sigma[i]).powi(2)).sum::<f64>())}
fn cost(poses:&[Pose],points:&[Point],marks:&[Mark],h:f64)->Option<f64>{
 let mut v=poses.iter().copied().map(prior_cost).sum::<f64>();
 for m in marks{v+=loss(projection(poses[m.camera].camera,points[m.point].xyz,m)?.0,h);}
 v.is_finite().then_some(v)
}
fn solve<const N:usize>(mut a:[[f64;N];N],mut b:[f64;N])->Option<[f64;N]>{
 let s=a.iter().flatten().map(|v|v.abs()).fold(0.,f64::max);if !s.is_finite()||s<=0.{return None;}
 for k in 0..N{
 let i=(k..N).max_by(|&i,&j|a[i][k].abs().total_cmp(&a[j][k].abs()))?;
 if a[i][k].abs()<=s*1e-12{return None;}a.swap(i,k);b.swap(i,k);
 for i in k+1..N{let q=a[i][k]/a[k][k];for j in k..N{a[i][j]-=q*a[k][j];}b[i]-=q*b[k];}
 }
 let mut x=[0.;N];for i in (0..N).rev(){x[i]=(b[i]-(i+1..N).map(|j|a[i][j]*x[j]).sum::<f64>())/a[i][i];}
 x.iter().all(|v|v.is_finite()).then_some(x)
}
fn accumulate<const N:usize>(a:&mut [[f64;N];N],b:&mut [f64;N],j:[f64;N],e:f64,w:f64){
 for i in 0..N{b[i]+=w*j[i]*e;for k in 0..N{a[i][k]+=w*j[i]*j[k];}}
}
fn delta<const N:usize>(mut a:[[f64;N];N],b:[f64;N],damping:f64)->Option<[f64;N]>{
 let s=(0..N).map(|i|a[i][i]).fold(0.,f64::max);if !s.is_finite()||s<=0.{return None;}
 for i in 0..N{a[i][i]+=damping*a[i][i].max(s*1e-9);}
 solve(a,b.map(|v|-v))
}
fn rotation(w:[f64;3])->[[f64;3];3]{
 let t=norm(w);let(a,b)=if t<1e-8{(1.-t*t/6.,0.5-t*t/24.)}else{(t.sin()/t,(1.-t.cos())/(t*t))};
 let [x,y,z]=w;let k=[[0.,-z,y],[z,0.,-x],[-y,x,0.]];
 std::array::from_fn(|i|std::array::from_fn(|j| (if i==j{1.}else{0.})+a*k[i][j]+b*(0..3).map(|l|k[i][l]*k[l][j]).sum::<f64>()))
}
fn move_pose(mut p:Pose,d:[f64;6])->Option<Pose>{
 if norm([d[3],d[4],d[5]])>0.5{return None;}
 for i in 0..3{p.camera.center[i]+=d[i];}
 let r=rotation([d[3],d[4],d[5]]);let old=p.camera.world_to_camera;
 p.camera.world_to_camera=std::array::from_fn(|i|std::array::from_fn(|j|(0..3).map(|k|r[i][k]*old[k][j]).sum()));
 p.camera.valid().then_some(p)
}
fn point_cost(p:[f64;3],poses:&[Pose],marks:&[Mark],ids:&[usize],h:f64)->Option<f64>{
 let mut v=0.;for &i in ids{let m=&marks[i];v+=loss(projection(poses[m.camera].camera,p,m)?.0,h);}v.is_finite().then_some(v)
}
fn pose_cost(p:Pose,points:&[Point],marks:&[Mark],ids:&[usize],h:f64)->Option<f64>{
 let mut v=prior_cost(p);for &i in ids{let m=&marks[i];v+=loss(projection(p.camera,points[m.point].xyz,m)?.0,h);}v.is_finite().then_some(v)
}
fn update_point(p:Point,poses:&[Pose],marks:&[Mark],ids:&[usize],h:f64)->Option<Point>{
 let(mut a,mut b)=([[0.;3];3],[0.;3]);let old=point_cost(p.xyz,poses,marks,ids,h)?;
 for &i in ids{let m=&marks[i];let c=poses[m.camera].camera;let(e,j,_)=projection(c,p.xyz,m)?;
 for k in 0..2{let row=std::array::from_fn(|l|(0..3).map(|v|j[k][v]*c.world_to_camera[v][l]).sum());
 accumulate(&mut a,&mut b,row,e[k],weight(e,h));}}
 for damping in [1e-4,1e-2,1.,100.]{if let Some(d)=delta(a,b,damping){
 let xyz=std::array::from_fn(|i|p.xyz[i]+d[i]);if point_cost(xyz,poses,marks,ids,h).is_some_and(|v|v<old){return Some(Point{xyz,..p});}
 }}None
}
fn update_pose(p:Pose,points:&[Point],marks:&[Mark],ids:&[usize],h:f64)->Option<Pose>{
 let(mut a,mut b)=([[0.;6];6],[0.;6]);let old=pose_cost(p,points,marks,ids,h)?;
 for &i in ids{let m=&marks[i];let(e,j,[x,y,z])=projection(p.camera,points[m.point].xyz,m)?;
 let rot=[[0.,z,-y],[-z,0.,x],[y,-x,0.]];
 for k in 0..2{let row=std::array::from_fn(|l|if l<3{-(0..3).map(|v|j[k][v]*p.camera.world_to_camera[v][l]).sum::<f64>()}else{(0..3).map(|v|j[k][v]*rot[v][l-3]).sum()});
 accumulate(&mut a,&mut b,row,e[k],weight(e,h));}}
 if let Some(pr)=p.prior{for i in 0..3{let mut row=[0.;6];row[i]=1./pr.sigma[i];accumulate(&mut a,&mut b,row,(p.camera.center[i]-pr.center[i])/pr.sigma[i],1.);}}
 for damping in [1e-4,1e-2,1.,100.]{if let Some(d)=delta(a,b,damping){if let Some(q)=move_pose(p,d){
 if pose_cost(q,points,marks,ids,h).is_some_and(|v|v<old){return Some(q);}
 }}}None
}
/// Transactional: cancellation or invalid input leaves caller buffers unchanged.
/// Every connected component must have two distinct fixed camera centers.
/// This is necessary, not sufficient, for full geometric observability.
pub fn optimize(poses:&mut[Pose],points:&mut[Point],marks:&[Mark],p:Policy,mut cancelled:impl FnMut()->bool)->Result<Report,Error>{
 if poses.len()>4096||points.len()>100_000||marks.len()>500_000||p.iterations>50{return Err(Error::Limit);}
 if poses.len()<2||points.is_empty()||marks.is_empty()||p.iterations==0||!p.huber.is_finite()||p.huber<=0.||!p.min_gain.is_finite()||p.min_gain<0.{return Err(Error::Invalid);}
 if !poses.iter().all(|p|p.camera.valid()&&p.prior.is_none_or(|q|q.center.iter().all(|v|v.is_finite())&&q.sigma.iter().all(|v|v.is_finite()&&*v>=1e-6)))||
 !points.iter().all(|p|p.xyz.iter().all(|v|v.is_finite())){return Err(Error::Invalid);}
 let(mut by_cam,mut by_pt)=(vec![Vec::new();poses.len()],vec![Vec::new();points.len()]);let mut seen=BTreeSet::new();
 for(i,m)in marks.iter().enumerate(){
 if m.camera>=poses.len()||m.point>=points.len()||!m.pixel.iter().all(|v|v.is_finite())||!m.sigma.is_finite()||m.sigma<1e-6||!seen.insert((m.camera,m.point)){return Err(Error::Invalid);}
 by_cam[m.camera].push(i);by_pt[m.point].push(i);
 }
 if by_cam.iter().enumerate().any(|(i,v)|v.len()<if poses[i].fixed{1}else{6})||by_pt.iter().any(|v|v.len()<2){return Err(Error::Unanchored);}
 let(mut visited_c,mut visited_p)=(vec![false;poses.len()],vec![false;points.len()]);
 for start in 0..poses.len(){if visited_c[start]{continue;}
 let mut queue=VecDeque::from([start]);visited_c[start]=true;let mut anchor:Option<[f64;3]>=None;let mut baseline=false;
 while let Some(c)=queue.pop_front(){
 if poses[c].fixed{if let Some(a)=anchor{baseline|=norm(sub(a,poses[c].camera.center))>1e-8;}else{anchor=Some(poses[c].camera.center);}}
 for &i in &by_cam[c]{let t=marks[i].point;if visited_p[t]{continue;}visited_p[t]=true;
 for &j in &by_pt[t]{let n=marks[j].camera;if !visited_c[n]{visited_c[n]=true;queue.push_back(n);}}}
 }if !baseline{return Err(Error::Unanchored);}
 }
 let(mut cameras,mut xyz)=(poses.to_vec(),points.to_vec());let initial=cost(&cameras,&xyz,marks,p.huber).ok_or(Error::Invalid)?;
 let(mut prev,mut accepted,mut sweeps)=(initial,0u64,0u32);
 for _ in 0..p.iterations{
 if cancelled(){return Err(Error::Cancelled);}
 for i in 0..xyz.len(){if i%128==0&&cancelled(){return Err(Error::Cancelled);}if !xyz[i].fixed{
 if let Some(q)=update_point(xyz[i],&cameras,marks,&by_pt[i],p.huber){xyz[i]=q;accepted+=1;}}}
 for i in 0..cameras.len(){if cancelled(){return Err(Error::Cancelled);}if !cameras[i].fixed{
 if let Some(q)=update_pose(cameras[i],&xyz,marks,&by_cam[i],p.huber){cameras[i]=q;accepted+=1;}}}
 let next=cost(&cameras,&xyz,marks,p.huber).ok_or(Error::Invalid)?;sweeps+=1;
 if next>prev+1e-10*prev.max(1.){return Err(Error::Invalid);}
 let gain=prev-next;prev=next;if gain<=p.min_gain{break;}
 }
 if cancelled(){return Err(Error::Cancelled);}
 poses.copy_from_slice(&cameras);points.copy_from_slice(&xyz);
 Ok(Report{initial_cost:initial,final_cost:prev,sweeps,accepted})
}
#[cfg(test)]mod tests{
 use super::*;
 fn camera(x:f64)->Camera{Camera{world_to_camera:[[1.,0.,0.],[0.,1.,0.],[0.,0.,1.]],center:[x,0.,0.],fx:200.,fy:200.,cx:100.,cy:100.}}
 fn fixture()->(Vec<Pose>,Vec<Point>,Vec<Mark>){
 let truth=[camera(-2.),camera(2.),camera(0.)];let mut poses:Vec<_>=truth.iter().enumerate().map(|(i,&camera)|Pose{camera,fixed:i<2,prior:None}).collect();
 poses[2].camera.center=[0.3,-0.2,0.1];poses[2].camera.world_to_camera=rotation([0.02,-0.03,0.01]);
 let mut pts=Vec::new();let mut marks=Vec::new();
 for i in 0..12{let p=[(i%4) as f64-1.5,(i/4) as f64-1.,9.+(i%3) as f64];
 pts.push(Point{xyz:[p[0]+0.1,p[1]-0.1,p[2]+0.3],fixed:false});
 for (j,c) in truth.iter().enumerate(){let d=sub(p,c.center);marks.push(Mark{camera:j,point:i,pixel:[c.fx*d[0]/d[2]+c.cx,c.fy*d[1]/d[2]+c.cy],sigma:1.});}}
 (poses,pts,marks)
 }
 fn policy()->Policy{Policy{iterations:50,huber:3.,min_gain:1e-10}}
 #[test]fn reduces_joint_objective_and_preserves_anchors(){
 let(mut p,mut x,m)=fixture();let fixed=p[0].camera.center;
 let r=optimize(&mut p,&mut x,&m,policy(),||false).unwrap();
 assert!(r.final_cost<r.initial_cost*0.02);assert!(r.accepted>0);assert_eq!(p[0].camera.center,fixed);
 assert!(norm(p[2].camera.center)<0.1);
 }
 #[test]fn unanchored_and_cancelled_leave_input_unchanged(){
 let(mut p,mut x,m)=fixture();let old=x[0].xyz;p[1].fixed=false;
 assert!(matches!(optimize(&mut p,&mut x,&m,policy(),||false),Err(Error::Unanchored)));
 p[1].fixed=true;assert!(matches!(optimize(&mut p,&mut x,&m,policy(),||true),Err(Error::Cancelled)));assert_eq!(x[0].xyz,old);}
}
