//! Weighted planar similarity for already levelled coordinates, plus Z offset.
//! Not 3D bundle adjustment, PPK, a CRS transform, or a geoid correction.
#[derive(Debug,Clone,Copy)]
pub struct Control{pub local:[f64;3],pub target:[f64;3],pub weight:f64}
#[derive(Debug,Clone,Copy)]
pub struct Transform{pub a:f64,pub b:f64,pub offset:[f64;3]}
#[derive(Debug,Clone,Copy,PartialEq,Eq)]
pub enum Error{Invalid,Degenerate,Limit}
#[derive(Debug,Clone,Copy)]
pub struct Residuals{pub count:usize,pub xy_rmse:f64,pub z_rmse:f64,pub xy_p95:f64,pub z_p95:f64}
fn valid(c:&Control)->bool{c.local.iter().chain(c.target.iter()).all(|v|v.is_finite())&&c.weight.is_finite()&&c.weight>0.}
impl Transform{
    pub fn apply(self,p:[f64;3])->[f64;3]{
        [self.a*p[0]-self.b*p[1]+self.offset[0],self.b*p[0]+self.a*p[1]+self.offset[1],p[2]+self.offset[2]]
    }
}
pub fn fit_planar(controls:&[Control])->Result<Transform,Error>{
    if controls.len()>100_000{return Err(Error::Limit);}
    if controls.len()<3||!controls.iter().all(valid){return Err(Error::Invalid);}
    let max_weight=controls.iter().map(|c|c.weight).fold(0.,f64::max);
    let(mut l,mut t,mut sw)=([0.;3],[0.;3],0.);
    for c in controls{let w=c.weight/max_weight;sw+=w;for i in 0..3{l[i]+=w*c.local[i];t[i]+=w*c.target[i];}}
    for i in 0..3{l[i]/=sw;t[i]/=sw;}
    let(mut dot,mut cross,mut den,mut xx,mut yy,mut xy)=(0.,0.,0.,0.,0.,0.);
    for c in controls{let w=c.weight/max_weight;let x=c.local[0]-l[0];let y=c.local[1]-l[1];
        let u=c.target[0]-t[0];let v=c.target[1]-t[1];
        dot+=w*(x*u+y*v);cross+=w*(x*v-y*u);den+=w*(x*x+y*y);
        xx+=w*x*x;yy+=w*y*y;xy+=w*x*y;}
    if !den.is_finite()||den<=1e-12||(xx/den)*(yy/den)-(xy/den).powi(2)<=1e-10{return Err(Error::Degenerate);}
    let a=dot/den;let b=cross/den;
    let r=Transform{a,b,offset:[t[0]-a*l[0]+b*l[1],t[1]-b*l[0]-a*l[1],t[2]-l[2]]};
    if !a.is_finite()||!b.is_finite()||a.hypot(b)<=1e-12||!r.offset.iter().all(|v|v.is_finite()){return Err(Error::Degenerate);}
    Ok(r)
}
/// Independent check points must be separated from fitting controls by the host.
pub fn residuals(t:Transform,check:&[Control])->Result<Residuals,Error>{
    if check.len()>100_000{return Err(Error::Limit);}
    if check.is_empty()||!check.iter().all(valid){return Err(Error::Invalid);}
    let(mut xy,mut z)=(Vec::with_capacity(check.len()),Vec::with_capacity(check.len()));
    for c in check{let p=t.apply(c.local);let dx=p[0]-c.target[0];let dy=p[1]-c.target[1];let dz=(p[2]-c.target[2]).abs();
        let d=dx.hypot(dy);if !d.is_finite()||!dz.is_finite(){return Err(Error::Invalid);}
        xy.push(d);z.push(dz);}
    let rmse=|v:&[f64]|{let max=v.iter().copied().fold(0.,f64::max);if max==0.{0.}else{max*(v.iter().map(|x|(x/max).powi(2)).sum::<f64>()/v.len() as f64).sqrt()}};
    let(xr,zr)=(rmse(&xy),rmse(&z));
    xy.sort_by(f64::total_cmp);z.sort_by(f64::total_cmp);
    let i=(95*check.len()).div_ceil(100)-1;
    Ok(Residuals{count:check.len(),xy_rmse:xr,z_rmse:zr,xy_p95:xy[i],z_p95:z[i]})
}
#[cfg(test)]mod tests{
    use super::*;
    fn c(x:f64,y:f64)->Control{Control{local:[x,y,1.],target:[10.-2.*y,20.+2.*x,4.],weight:1.}}
    #[test]fn solves_rotation_scale_translation(){let t=fit_planar(&[c(0.,0.),c(1.,0.),c(0.,1.)]).unwrap();
        let r=residuals(t,&[c(4.,5.)]).unwrap();assert!(r.xy_rmse<1e-10&&r.z_rmse<1e-10);}
    #[test]fn rejects_collinear(){assert!(matches!(fit_planar(&[c(0.,0.),c(1.,0.),c(2.,0.)]),Err(Error::Degenerate)));}
}
