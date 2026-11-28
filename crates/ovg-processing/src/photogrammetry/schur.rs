//! Matrix-free Schur complement for damped camera/landmark normal equations.
//! U: 6x6 camera diagonal blocks, V: 3x3 landmark blocks, W: 6x3 cross blocks.
//! Caller supplies correctly linearized, anchored, symmetric positive definite system.
//! Memory is linear in cameras, landmarks and observation cross blocks.
pub type C6=[f64;6];
pub type P3=[f64;3];
pub type M6=[[f64;6];6];
pub type M3=[[f64;3];3];
pub type M63=[[f64;3];6];
#[derive(Debug,Clone)]
pub struct Link{pub camera:usize,pub cross:M63}
#[derive(Debug,Clone)]
pub struct Landmark{pub normal:M3,pub rhs:P3,pub links:Vec<Link>}
pub struct System<'a>{pub camera:&'a[M6],pub rhs:&'a[C6],pub landmarks:&'a[Landmark]}
#[derive(Debug,Clone,Copy)]
pub struct Policy{pub max_iterations:u32,pub relative_tolerance:f64,pub max_bytes:u64}
#[derive(Debug)]
pub struct Solution{pub camera:Vec<C6>,pub landmark:Vec<P3>,pub iterations:u32,pub residual:f64}
#[derive(Debug,Clone,Copy,PartialEq,Eq)]
pub enum Error{Invalid,Limit,Indefinite,NonConverged,Cancelled}
fn chol<const N:usize>(a:[[f64;N];N])->Result<[[f64;N];N],Error>{
 let mut l=[[0.;N];N];
 let scale=(0..N).map(|i|a[i][i].abs()).fold(0.,f64::max);
 if !scale.is_finite()||scale<=0.{return Err(Error::Indefinite);}
 for i in 0..N{for j in 0..=i{
 if !a[i][j].is_finite()||!a[j][i].is_finite()||(a[i][j]-a[j][i]).abs()>scale*1e-10{return Err(Error::Invalid);}
 let s=a[i][j]-(0..j).map(|k|l[i][k]*l[j][k]).sum::<f64>();
 if i==j{if !s.is_finite()||s<=scale*1e-14{return Err(Error::Indefinite);}l[i][j]=s.sqrt();}
 else{l[i][j]=s/l[j][j];}
 }}Ok(l)
}
fn solve<const N:usize>(l:&[[f64;N];N],b:[f64;N])->[f64;N]{
 let mut y=[0.;N];let mut x=[0.;N];
 for i in 0..N{y[i]=(b[i]-(0..i).map(|j|l[i][j]*y[j]).sum::<f64>())/l[i][i];}
 for i in (0..N).rev(){x[i]=(y[i]-(i+1..N).map(|j|l[j][i]*x[j]).sum::<f64>())/l[i][i];}
 x
}
fn cross_t(l:&Link,c:C6,z:&mut P3){
 for j in 0..3{for i in 0..6{z[j]+=l.cross[i][j]*c[i];}}
}
fn subtract_cross(l:&Link,p:P3,c:&mut C6){
 for i in 0..6{for j in 0..3{c[i]-=l.cross[i][j]*p[j];}}
}
fn apply(s:&System<'_>,factors:&[M3],x:&[C6],out:&mut[C6]){
 for (i,a) in s.camera.iter().enumerate(){out[i]=std::array::from_fn(|j|(0..6).map(|k|a[j][k]*x[i][k]).sum());}
 for (p,v) in s.landmarks.iter().zip(factors){
 let mut z=[0.;3];for l in &p.links{cross_t(l,x[l.camera],&mut z);}
 let z=solve(v,z);
 for l in &p.links{subtract_cross(l,z,&mut out[l.camera]);}
 }
}
fn dot(a:&[C6],b:&[C6])->f64{a.iter().zip(b).map(|(a,b)|(0..6).map(|i|a[i]*b[i]).sum::<f64>()).sum()}
fn precondition(f:&[M6],r:&[C6],z:&mut[C6]){for ((l,r),z)in f.iter().zip(r).zip(z){*z=solve(l,*r);}}
/// Solves S dc = bc - W V^-1 bp by PCG, then dp = V^-1(bp - W^T dc).
/// Uses camera-block Jacobi preconditioner, no quadratic camera matrix allocation.
pub fn solve_system(s:System<'_>,p:Policy,mut cancelled:impl FnMut()->bool)->Result<Solution,Error>{
 let n=s.camera.len();let m=s.landmarks.len();
 if n==0||s.rhs.len()!=n||p.max_iterations==0||!p.relative_tolerance.is_finite()||p.relative_tolerance<=0.||p.relative_tolerance>=1.{return Err(Error::Invalid);}
 if n>100_000||m>2_000_000||p.max_iterations>2000{return Err(Error::Limit);}
 let links=s.landmarks.iter().try_fold(0usize,|n,p|n.checked_add(p.links.len())).ok_or(Error::Limit)?;
 // Internal allocation estimate; caller separately accounts input blocks and allocator overhead.
 let bytes=(n as u64).checked_mul(1024).and_then(|v|v.checked_add(m as u64*160)).ok_or(Error::Limit)?;
 if links>10_000_000||bytes>p.max_bytes{return Err(Error::Limit);}
 let mut factors=Vec::with_capacity(m);
 let mut rhs=s.rhs.to_vec();
 if !rhs.iter().flatten().all(|v|v.is_finite()){return Err(Error::Invalid);}
 let mut diag=s.camera.to_vec();
 for (idx,a)in s.landmarks.iter().enumerate(){
 if idx%256==0&&cancelled(){return Err(Error::Cancelled);}
 if !a.rhs.iter().all(|v|v.is_finite()){return Err(Error::Invalid);}
 let l=chol(a.normal)?;let z=solve(&l,a.rhs);
 let mut last=None;
 for w in &a.links{
 if w.camera>=n||last.is_some_and(|i|i>=w.camera)||!w.cross.iter().flatten().all(|v|v.is_finite()){return Err(Error::Invalid);}
 last=Some(w.camera);subtract_cross(w,z,&mut rhs[w.camera]);
 let q=w.cross.map(|row|solve(&l,row));
 for i in 0..6{for j in 0..6{diag[w.camera][i][j]-=(0..3).map(|k|q[i][k]*w.cross[j][k]).sum::<f64>();}}
 }
 factors.push(l);
 }
 let f:Vec<M6>=diag.into_iter().map(chol).collect::<Result<_,_>>()?;
 let mut x=vec![[0.;6];n];let mut r=rhs.clone();let mut z=vec![[0.;6];n];let mut ap=vec![[0.;6];n];
 let base=dot(&rhs,&rhs).sqrt();if !base.is_finite(){return Err(Error::Invalid);}
 let mut residual=base;let mut iterations=0;
 if base>0.{
 precondition(&f,&r,&mut z);let mut d=z.clone();let mut rz=dot(&r,&z);
 for k in 0..p.max_iterations{
 if cancelled(){return Err(Error::Cancelled);}
 apply(&s,&factors,&d,&mut ap);
 let denom=dot(&d,&ap);if !denom.is_finite()||denom<=0.||!rz.is_finite()||rz<=0.{return Err(Error::Indefinite);}
 let alpha=rz/denom;if !alpha.is_finite(){return Err(Error::Indefinite);}
 for i in 0..n{for j in 0..6{x[i][j]+=alpha*d[i][j];r[i][j]-=alpha*ap[i][j];}}
 iterations=k+1;residual=dot(&r,&r).sqrt();if !residual.is_finite(){return Err(Error::Indefinite);}
 if residual<=base*p.relative_tolerance{break;}
 precondition(&f,&r,&mut z);let next=dot(&r,&z);if !next.is_finite()||next<=0.{return Err(Error::Indefinite);}
 let beta=next/rz;for i in 0..n{for j in 0..6{d[i][j]=z[i][j]+beta*d[i][j];}}rz=next;
 }
 // Recompute the true residual before accepting recursive PCG convergence.
 apply(&s,&factors,&x,&mut ap);
 for i in 0..n{for j in 0..6{r[i][j]=rhs[i][j]-ap[i][j];}}
 residual=dot(&r,&r).sqrt();if !residual.is_finite()||residual>base*p.relative_tolerance{return Err(Error::NonConverged);}
 }
 let mut landmark=Vec::with_capacity(m);
 for (i,(a,l))in s.landmarks.iter().zip(&factors).enumerate(){
 if i%256==0&&cancelled(){return Err(Error::Cancelled);}
 let mut v=[0.;3];for w in &a.links{cross_t(w,x[w.camera],&mut v);}
 let d=solve(l,std::array::from_fn(|i|a.rhs[i]-v[i]));if !d.iter().all(|v|v.is_finite()){return Err(Error::Indefinite);}landmark.push(d);
 }
 if cancelled(){return Err(Error::Cancelled);}
 Ok(Solution{camera:x,landmark,iterations,residual})
}
#[cfg(test)]mod tests{
 use super::*;
 fn diag<const N:usize>(d:f64)->[[f64;N];N]{std::array::from_fn(|i|std::array::from_fn(|j|if i==j{d}else{0.}))}
 fn fixture()->(Vec<M6>,Vec<C6>,Vec<Landmark>){
 let mut w=[[0.;3];6];w[0][0]=1.;
 let a=vec![diag(4.),diag(4.)];let mut b=vec![[0.;6];2];b[0][0]=3.;b[1][0]=7.;
 // Known solution c0=1,c1=2,p=-1: 4*c0+p=3,4*c1+p=7,c0+c1+3*p=0.
 let p=Landmark{normal:diag(3.),rhs:[0.;3],links:vec![Link{camera:0,cross:w},Link{camera:1,cross:w}]};(a,b,vec![p])
 }
 fn policy()->Policy{Policy{max_iterations:20,relative_tolerance:1e-10,max_bytes:100_000}}
 #[test]fn solves_coupled_blocks_without_dense_matrix(){
 let(a,b,p)=fixture();let q=solve_system(System{camera:&a,rhs:&b,landmarks:&p},policy(),||false).unwrap();
 assert!((q.camera[0][0]-1.).abs()<1e-9);assert!((q.camera[1][0]-2.).abs()<1e-9);assert!((q.landmark[0][0]+1.).abs()<1e-9);}
 #[test]fn rejects_indefinite_and_cancel(){
 let(a,b,mut p)=fixture();assert!(matches!(solve_system(System{camera:&a,rhs:&b,landmarks:&p},policy(),||true),Err(Error::Cancelled)));
 p[0].normal[0][0]=-1.;assert!(matches!(solve_system(System{camera:&a,rhs:&b,landmarks:&p},policy(),||false),Err(Error::Indefinite)));}
}
