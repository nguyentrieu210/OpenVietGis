//! Weighted 3D similarity fit (Horn quaternion method), not bundle adjustment.
//! Source/target must be Cartesian in known units; target projected CRS and
//! vertical datum are host responsibilities. Independent checks required.
use super::{colmap_model::Alignment,survey::Control};
#[derive(Debug,Clone,Copy,PartialEq,Eq)]pub enum Error{Invalid,Degenerate,Limit,Convergence}
fn dot(a:[f64;3],b:[f64;3])->f64{(0..3).map(|i|a[i]*b[i]).sum()}
fn cross(a:[f64;3],b:[f64;3])->[f64;3]{[a[1]*b[2]-a[2]*b[1],a[2]*b[0]-a[0]*b[2],a[0]*b[1]-a[1]*b[0]]}
fn norm(a:[f64;3])->f64{a[0].hypot(a[1]).hypot(a[2])}
fn sub(a:[f64;3],b:[f64;3])->[f64;3]{std::array::from_fn(|i|a[i]-b[i])}
fn mul(r:[[f64;3];3],x:[f64;3])->[f64;3]{r.map(|v|dot(v,x))}
fn valid(c:&Control)->bool{c.local.iter().chain(c.target.iter()).all(|v|v.is_finite())&&c.weight.is_finite()&&c.weight>0.}
/// Symmetric Jacobi eigensolver; use largest algebraic eigenvalue, not largest magnitude.
fn eigen(mut a:[[f64;4];4])->Result<[f64;4],Error>{
 let mut v=[[0.;4];4];for i in 0..4{v[i][i]=1.;}
 for _ in 0..96{
 let(mut p,mut q,mut m)=(0,1,0.);
 for i in 0..4{for j in i+1..4{if a[i][j].abs()>m{p=i;q=j;m=a[i][j].abs();}}}
 let scale=(0..4).map(|i|a[i][i].abs()).fold(1.,f64::max);
 if m<=1e-14*scale{
 let mut order=[0usize,1,2,3];order.sort_by(|&i,&j|a[j][j].total_cmp(&a[i][i]));
 if a[order[0]][order[0]]-a[order[1]][order[1]]<=1e-10*scale{return Err(Error::Degenerate);}
 return Ok(std::array::from_fn(|i|v[i][order[0]]));
 }
 let theta=0.5*(2.*a[p][q]).atan2(a[q][q]-a[p][p]);let(c,s)=(theta.cos(),theta.sin());
 let(app,aqq,apq)=(a[p][p],a[q][q],a[p][q]);
 for k in 0..4{if k!=p&&k!=q{
 let(x,y)=(a[k][p],a[k][q]);a[k][p]=c*x-s*y;a[p][k]=a[k][p];a[k][q]=s*x+c*y;a[q][k]=a[k][q];
 }}
 a[p][p]=c*c*app-2.*s*c*apq+s*s*aqq;a[q][q]=s*s*app+2.*s*c*apq+c*c*aqq;a[p][q]=0.;a[q][p]=0.;
 for row in &mut v{let(x,y)=(row[p],row[q]);row[p]=c*x-s*y;row[q]=s*x+c*y;}
 }
 Err(Error::Convergence)
}
pub fn fit(controls:&[Control])->Result<Alignment,Error>{
 if controls.len()>100_000{return Err(Error::Limit);}
 if controls.len()<3||!controls.iter().all(valid){return Err(Error::Invalid);}
 let mw=controls.iter().map(|c|c.weight).fold(0.,f64::max);
 let mut l=[0.;3];let mut t=[0.;3];let mut sw=0.;
 // Center relative to first control to reduce large projected-origin cancellation.
 let l0=controls[0].local;let t0=controls[0].target;
 for c in controls{let w=c.weight/mw;sw+=w;for i in 0..3{l[i]+=w*(c.local[i]-l0[i]);t[i]+=w*(c.target[i]-t0[i]);}}
 for i in 0..3{l[i]=l0[i]+l[i]/sw;t[i]=t0[i]+t[i]/sw;}
 if !l.iter().chain(t.iter()).all(|v|v.is_finite()){return Err(Error::Invalid);}
 let ls=controls.iter().map(|c|norm(sub(c.local,l))).fold(0.,f64::max);
 let ts=controls.iter().map(|c|norm(sub(c.target,t))).fold(0.,f64::max);
 if !ls.is_finite()||!ts.is_finite()||ls<=1e-12||ts<=1e-12{return Err(Error::Degenerate);}
 let mut h=[[0.;3];3];let mut den=0.;let mut anchor=[0.;3];let mut anchor_norm=0.;
 for c in controls{
 let x=sub(c.local,l).map(|v|v/ls);let y=sub(c.target,t).map(|v|v/ts);let w=(c.weight/mw)/sw;
 let n=norm(x);if n>anchor_norm{anchor=x;anchor_norm=n;}
 den+=w*dot(x,x);for i in 0..3{for j in 0..3{h[i][j]+=w*x[i]*y[j];}}
 }
 let spread=controls.iter().map(|c|{let x=sub(c.local,l).map(|v|v/ls);(c.weight/mw)/sw*dot(cross(anchor,x),cross(anchor,x))}).sum::<f64>();
 if den<=1e-12||spread<=1e-10*den{return Err(Error::Degenerate);}
 let [[xx,xy,xz],[yx,yy,yz],[zx,zy,zz]]=h;
 let q=eigen([[xx+yy+zz,yz-zy,zx-xz,xy-yx],
 [yz-zy,xx-yy-zz,xy+yx,zx+xz],[zx-xz,xy+yx,-xx+yy-zz,yz+zy],[xy-yx,zx+xz,yz+zy,-xx-yy+zz]])?;
 let [w,x,y,z]=q;let r=[[1.-2.*(y*y+z*z),2.*(x*y-z*w),2.*(x*z+y*w)],
 [2.*(x*y+z*w),1.-2.*(x*x+z*z),2.*(y*z-x*w)],[2.*(x*z-y*w),2.*(y*z+x*w),1.-2.*(x*x+y*y)]];
 let num=controls.iter().map(|c|{let x=sub(c.local,l).map(|v|v/ls);let y=sub(c.target,t).map(|v|v/ts);
 (c.weight/mw)/sw*dot(mul(r,x),y)}).sum::<f64>();
 let scale=(num/den)*(ts/ls);if !scale.is_finite()||scale<=0.{return Err(Error::Degenerate);}
 let rl=mul(r,l);let offset=std::array::from_fn(|i|t[i]-scale*rl[i]);
 if !offset.iter().all(|v|v.is_finite()){return Err(Error::Invalid);}
 Ok(Alignment{scale,rotation:r,offset})
}
#[derive(Debug,Clone,Copy)]pub struct Check{pub count:usize,pub xy_rmse:f64,pub z_rmse:f64,pub max_3d:f64}
pub fn check(a:Alignment,points:&[Control])->Result<Check,Error>{
 if points.len()>100_000{return Err(Error::Limit);}
 if points.is_empty()||!points.iter().all(valid)||!a.scale.is_finite()||a.scale<=0.{return Err(Error::Invalid);}
 let(mut xy,mut z,mut max):(f64,f64,f64)=(0.,0.,0.);
 for c in points{
 let p=mul(a.rotation,c.local);let d:[f64;3]=std::array::from_fn(|i|a.offset[i]+a.scale*p[i]-c.target[i]);
 let h=d[0].hypot(d[1]);let n=norm(d);
 if !n.is_finite(){return Err(Error::Invalid);}
 xy=xy.hypot(h);z=z.hypot(d[2]);max=max.max(n);
 }
 let n=(points.len() as f64).sqrt();
 Ok(Check{count:points.len(),xy_rmse:xy/n,z_rmse:z/n,max_3d:max})
}
#[cfg(test)]mod tests{
 use super::*;
 fn c(x:[f64;3])->Control{Control{local:x,target:[500000.+2.*x[2],1200000.+2.*x[0],100.+2.*x[1]],weight:1.}}
 #[test]fn recovers_3d_similarity_and_independent_check(){
 let a=fit(&[c([0.;3]),c([1.,0.,0.]),c([0.,1.,0.]),c([0.,0.,1.])]).unwrap();
 assert!((a.scale-2.).abs()<1e-8);
 let q=check(a,&[c([4.,5.,6.]),c([-1.,2.,3.])]).unwrap();
 assert!(q.max_3d<1e-7);}
 #[test]fn rejects_collinear_and_invalid(){
 assert!(matches!(fit(&[c([0.;3]),c([1.,0.,0.]),c([2.,0.,0.])]),Err(Error::Degenerate)));
 assert!(matches!(fit(&[]),Err(Error::Invalid)));}
}
