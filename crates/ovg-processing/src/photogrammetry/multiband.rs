//! Bounded two-image Laplacian pyramid blend in linear light.
//! Full-coverage overlaps only. Nodata is explicitly rejected to avoid dark halos.
use super::seam::{self,Error};
#[derive(Debug,Clone,Copy)]
pub struct Policy{pub levels:u8,pub max_bytes:usize,pub max_pixels:usize}
struct Plane{w:usize,h:usize,v:Vec<f32>}
fn down(p:&Plane)->Plane{
 let w=p.w.div_ceil(2);let h=p.h.div_ceil(2);let mut v=vec![0.;w*h];
 let k=[1.,4.,6.,4.,1.];
 for y in 0..h{for x in 0..w{
 let mut s=0.;
 for j in 0..5{for i in 0..5{
 let xx=(2*x+i).saturating_sub(2).min(p.w-1);let yy=(2*y+j).saturating_sub(2).min(p.h-1);
 s+=k[i]*k[j]*p.v[yy*p.w+xx];
 }}v[y*w+x]=s/256.;
 }}Plane{w,h,v}
}
fn up_at(p:&Plane,x:usize,y:usize)->f32{
 let x0=(x/2).min(p.w-1);let y0=(y/2).min(p.h-1);
 let x1=(x0+1).min(p.w-1);let y1=(y0+1).min(p.h-1);
 let a=(x%2)as f32*0.5;let b=(y%2)as f32*0.5;
 (1.-b)*((1.-a)*p.v[y0*p.w+x0]+a*p.v[y0*p.w+x1])+b*((1.-a)*p.v[y1*p.w+x0]+a*p.v[y1*p.w+x1])
}
fn pyramid(first:Plane,levels:u8)->Vec<Plane>{
 let mut v=vec![first];
 for _ in 1..levels{let p=v.last().unwrap();if p.w==1&&p.h==1{break;}v.push(down(p));}v
}
/// Working memory cap includes all owned image pyramids, weights and output;
/// excludes caller-owned RGBA and mask. Processes channels independently.
pub fn blend(a:&[u8],b:&[u8],weights:&[f32],width:u32,height:u32,offset:[i16;3],p:Policy,
 mut cancelled:impl FnMut()->bool)->Result<Vec<u8>,Error>{
 let(w,h)=(width as usize,height as usize);let n=w.checked_mul(h).ok_or(Error::Limit)?;
 let bytes=n.checked_mul(4).ok_or(Error::Limit)?;
 if w==0||h==0||p.levels==0||p.levels>8||a.len()!=bytes||b.len()!=bytes||weights.len()!=n||
 offset.iter().any(|v|v.unsigned_abs()>64)||weights.iter().any(|v|!v.is_finite()||!(0.0..=1.0).contains(v)){
 return Err(Error::Invalid);}
 if n>p.max_pixels||n>16_000_000||n.checked_mul(80).ok_or(Error::Limit)?>p.max_bytes{return Err(Error::Limit);}
 if a.chunks_exact(4).chain(b.chunks_exact(4)).any(|v|v[3]!=255){return Err(Error::InsufficientOverlap);}
 if cancelled(){return Err(Error::Cancelled);}
 let m=pyramid(Plane{w,h,v:weights.to_vec()},p.levels);let mut out=vec![255;bytes];
 for c in 0..3{
 if cancelled(){return Err(Error::Cancelled);}
 let aa=pyramid(Plane{w,h,v:a.chunks_exact(4).map(|q|seam::linear(q[c])as f32).collect()},p.levels);
 let bb=pyramid(Plane{w,h,v:b.chunks_exact(4).map(|q|seam::linear((i16::from(q[c])+offset[c]).clamp(0,255)as u8)as f32).collect()},p.levels);
 let last=m.len()-1;
 let mut current=Plane{w:m[last].w,h:m[last].h,v:aa[last].v.iter().zip(&bb[last].v).zip(&m[last].v).map(|((a,b),t)|(1.-t)*a+t*b).collect()};
 for l in (0..last).rev(){
 if cancelled(){return Err(Error::Cancelled);}
 let q=&m[l];let mut v=vec![0.;q.w*q.h];
 for y in 0..q.h{if y%32==0&&cancelled(){return Err(Error::Cancelled);}for x in 0..q.w{
 let i=y*q.w+x;let la=aa[l].v[i]-up_at(&aa[l+1],x,y);let lb=bb[l].v[i]-up_at(&bb[l+1],x,y);
 v[i]=up_at(&current,x,y)+(1.-q.v[i])*la+q.v[i]*lb;
 }}current=Plane{w:q.w,h:q.h,v};
 }
 for(i,v)in current.v.into_iter().enumerate(){out[4*i+c]=seam::srgb(f64::from(v));}
 }
 if cancelled(){return Err(Error::Cancelled);}Ok(out)
}
#[cfg(test)]mod tests{
 use super::*;
 fn p()->Policy{Policy{levels:4,max_bytes:100_000,max_pixels:1000}}
 #[test]fn endpoints_and_odd_shapes(){
 let a:Vec<u8>=(0..35).flat_map(|i|[i as u8,20,100,255]).collect();let b=vec![200,100,30,255].repeat(35);
 assert_eq!(blend(&a,&b,&[0.;35],7,5,[0;3],p(),||false).unwrap(),a);
 assert_eq!(blend(&a,&b,&[1.;35],7,5,[0;3],p(),||false).unwrap(),b);
 }
 #[test]fn uniform_linear_light_and_nodata(){
 let a=vec![0,0,0,255].repeat(9);let mut b=vec![255,255,255,255].repeat(9);
 let q=blend(&a,&b,&[0.5;9],3,3,[0;3],p(),||false).unwrap();assert_eq!(q,vec![188,188,188,255].repeat(9));
 b[3]=0;assert_eq!(blend(&a,&b,&[0.5;9],3,3,[0;3],p(),||false),Err(Error::InsufficientOverlap));
 }
}
