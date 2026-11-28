//! Conservative multi-scale lower-envelope ground candidate filter.
//! Rejects elevated cells relative to supported neighbouring lower cells.
//! It cannot see ground beneath opaque canopy, and cannot certify bare earth.
use super::ortho::Grid;
#[derive(Debug,Clone,Copy)]pub struct Policy{pub max_radius:u8,pub min_neighbors:u8,pub max_slope:f64,pub vertical_tolerance:f64,pub max_work_bytes:u64,pub max_comparisons:u64}
#[derive(Debug,Clone,Copy,PartialEq,Eq)]pub enum Error{Invalid,Limit,Cancelled}
#[derive(Debug)]pub struct ResultGrid{pub elevations:Vec<f32>,pub accepted:u64,pub rejected:u64,pub unsupported:u64}
/// Samples eight directions at powers-of-two radii. A supported radius must
/// have enough finite neighbours; no synthetic heights are introduced.
/// Input must be minimum supported cell elevations in metres, not a DSM max.
pub fn filter(g:Grid,lower:&[f32],p:Policy,mut cancelled:impl FnMut()->bool)->Result<ResultGrid,Error>{
 if g.width==0||g.height==0||!g.gsd.is_finite()||g.gsd<=0.||p.max_radius==0||p.max_radius>64||
 p.min_neighbors<3||p.min_neighbors>8||!p.max_slope.is_finite()||p.max_slope<0.||p.max_slope>5.||
 !p.vertical_tolerance.is_finite()||p.vertical_tolerance<=0.||p.vertical_tolerance>100.||
 lower.iter().any(|x|x.is_infinite()){return Err(Error::Invalid);}
 let n=u64::from(g.width)*u64::from(g.height);
 if n!=lower.len()as u64||n>16_000_000||n.checked_mul(4).ok_or(Error::Limit)?>p.max_work_bytes{return Err(Error::Limit);}
 let(w,h)=(i64::from(g.width),i64::from(g.height));
 let dirs=[(-1,-1),(0,-1),(1,-1),(-1,0),(1,0),(-1,1),(0,1),(1,1)];
 let mut out=vec![f32::NAN;lower.len()];
 let(mut accepted,mut rejected,mut unsupported,mut comparisons)=(0u64,0u64,0u64,0u64);
 for y in 0..h{
 if cancelled(){return Err(Error::Cancelled);}
 for x in 0..w{
 let i=(y*w+x)as usize;let z=lower[i];if z.is_nan(){continue;}
 let(mut supported,mut elevated)=(false,false);let mut radius=1i64;
 while radius<=i64::from(p.max_radius){
 let(mut count,mut low_votes)=(0u8,0u8);
 for (dx,dy)in dirs{
 comparisons=comparisons.checked_add(1).ok_or(Error::Limit)?;if comparisons>p.max_comparisons{return Err(Error::Limit);}
 let xx=x+dx*radius;let yy=y+dy*radius;
 if xx<0||yy<0||xx>=w||yy>=h{continue;}
 let v=lower[(yy*w+xx)as usize];if !v.is_finite(){continue;}count+=1;
 let distance=(dx as f64).hypot(dy as f64)*radius as f64*g.gsd;
 if f64::from(z)-f64::from(v)>p.vertical_tolerance+p.max_slope*distance{low_votes+=1;}
 }
 if count>=p.min_neighbors{
 supported=true;
 // At least two low neighbours reduce isolated low-outlier sensitivity.
 if low_votes>=2{elevated=true;break;}
 }
 radius*=2;
 }
 if elevated{rejected+=1;}else if !supported{unsupported+=1;}else{out[i]=z;accepted+=1;}
 }
 }
 if cancelled(){return Err(Error::Cancelled);}Ok(ResultGrid{elevations:out,accepted,rejected,unsupported})
}
#[cfg(test)]mod tests{
 use super::*;
 fn grid()->Grid{Grid{origin_x:0.,origin_y:7.,gsd:1.,width:7,height:7}}
 fn policy()->Policy{Policy{max_radius:4,min_neighbors:3,max_slope:0.5,vertical_tolerance:0.3,max_work_bytes:1024,max_comparisons:10000}}
 #[test]fn rejects_elevated_roof_keeps_supported_slope(){
 let mut z:Vec<f32>=(0..49).map(|i|(i%7)as f32*0.2).collect();z[24]+=5.;
 let r=filter(grid(),&z,policy(),||false).unwrap();
 assert!(r.elevations[24].is_nan());assert_eq!(r.rejected,1);assert_eq!(r.accepted,48);
 }
 #[test]fn sparse_data_remains_unknown(){
 let mut z=vec![f32::NAN;49];z[24]=10.;let r=filter(grid(),&z,policy(),||false).unwrap();
 assert_eq!(r.unsupported,1);assert_eq!(r.accepted,0);assert!(r.elevations.iter().all(|v|v.is_nan()));
 assert!(matches!(filter(grid(),&z,policy(),||true),Err(Error::Cancelled)));
 }
}
