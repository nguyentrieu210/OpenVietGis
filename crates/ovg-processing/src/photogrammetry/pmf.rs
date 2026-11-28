//! Progressive morphological opening on a lower-envelope metric grid.
//! Separable monotonic deque min/max passes: O(cells) per window.
//! Strict nodata support and no interpolation; output is a DTM candidate.
use std::collections::VecDeque;
use super::ortho::Grid;
#[derive(Debug,Clone,Copy)]pub struct Policy{pub max_radius:u16,pub initial_threshold_m:f64,pub slope:f64,pub max_threshold_m:f64,pub max_work_bytes:u64,pub max_cell_passes:u64}
#[derive(Debug,Clone,Copy,PartialEq,Eq)]pub enum Error{Invalid,Limit,Cancelled}
#[derive(Debug)]pub struct ResultGrid{pub elevations:Vec<f32>,pub accepted:u64,pub rejected:u64,pub unsupported:u64}
// Every sample in the clipped window must be finite. Edges are clipped,
// never extrapolated. A full separable opening therefore has explicit support.
fn line(input:&[f32],output:&mut[f32],r:usize,min:bool){
 let mut q=VecDeque::<usize>::new();let mut end=0usize;let mut bad=0usize;let mut left=0usize;
 for i in 0..input.len(){
 let lo=i.saturating_sub(r);let hi=i.saturating_add(r).saturating_add(1).min(input.len());
 while left<lo{if !input[left].is_finite(){bad-=1;}left+=1;}
 while q.front().is_some_and(|&j|j<lo){q.pop_front();}
 while end<hi{
 let v=input[end];if !v.is_finite(){bad+=1;}else{
 while q.back().is_some_and(|&j|if min{input[j]>=v}else{input[j]<=v}){q.pop_back();}
 q.push_back(end);
 }end+=1;
 }
 output[i]=if bad==0{q.front().map_or(f32::NAN,|&j|input[j])}else{f32::NAN};
 }
}
fn pass(src:&[f32],dst:&mut[f32],w:usize,h:usize,r:usize,horizontal:bool,min:bool,stop:&mut impl FnMut()->bool)->Result<(),Error>{
 if horizontal{
 for y in 0..h{if stop(){return Err(Error::Cancelled);}line(&src[y*w..(y+1)*w],&mut dst[y*w..(y+1)*w],r,min);}
 }else{
 let mut a=vec![0.;h];let mut b=vec![0.;h];
 for x in 0..w{if stop(){return Err(Error::Cancelled);}for y in 0..h{a[y]=src[y*w+x];}
 line(&a,&mut b,r,min);for y in 0..h{dst[y*w+x]=b[y];}}
 }Ok(())
}
pub fn filter(g:Grid,lower:&[f32],p:Policy,mut stop:impl FnMut()->bool)->Result<ResultGrid,Error>{
 if g.width==0||g.height==0||!g.gsd.is_finite()||g.gsd<=0.||p.max_radius==0||p.max_radius>256||
 !p.initial_threshold_m.is_finite()||p.initial_threshold_m<=0.||!p.slope.is_finite()||p.slope<0.||p.slope>5.||
 !p.max_threshold_m.is_finite()||p.max_threshold_m<p.initial_threshold_m||lower.iter().any(|x|x.is_infinite()){return Err(Error::Invalid);}
 let(w,h)=(g.width as usize,g.height as usize);let n=w.checked_mul(h).ok_or(Error::Limit)?;
 if n!=lower.len()||n>16_000_000{return Err(Error::Limit);}
 let mut radii=vec![1usize];while *radii.last().unwrap()<usize::from(p.max_radius){
 radii.push((radii.last().unwrap()*2).min(usize::from(p.max_radius)));
 }
 let work=(n as u64).checked_mul(22).and_then(|v|v.checked_add((w.max(h)as u64)*24)).ok_or(Error::Limit)?;
 if work>p.max_work_bytes||(n as u64).checked_mul(4*radii.len()as u64).ok_or(Error::Limit)?>p.max_cell_passes{return Err(Error::Limit);}
 let mut a=vec![0.;n];let mut b=vec![0.;n];let mut reject=vec![false;n];let mut support=vec![false;n];
 let mut prev=0usize;
 for r in radii{
 pass(lower,&mut a,w,h,r,true,true,&mut stop)?;
 pass(&a,&mut b,w,h,r,false,true,&mut stop)?;
 pass(&b,&mut a,w,h,r,true,false,&mut stop)?;
 pass(&a,&mut b,w,h,r,false,false,&mut stop)?;
 let threshold=(p.initial_threshold_m+p.slope*g.gsd*(2*(r-prev))as f64).min(p.max_threshold_m);
 for i in 0..n{
 if i%65536==0&&stop(){return Err(Error::Cancelled);}
 if lower[i].is_finite()&&b[i].is_finite(){support[i]=true;if f64::from(lower[i])-f64::from(b[i])>threshold{reject[i]=true;}}
 }prev=r;
 }
 let(mut accepted,mut rejected,mut unsupported)=(0u64,0u64,0u64);
 let mut elevations=vec![f32::NAN;n];
 for i in 0..n{
 if lower[i].is_nan(){continue;}if reject[i]{rejected+=1;}else if !support[i]{unsupported+=1;}else{elevations[i]=lower[i];accepted+=1;}
 }
 if stop(){return Err(Error::Cancelled);}Ok(ResultGrid{elevations,accepted,rejected,unsupported})
}
#[cfg(test)]mod tests{
 use super::*;
 fn g()->Grid{Grid{origin_x:0.,origin_y:9.,gsd:1.,width:9,height:9}}
 fn p()->Policy{Policy{max_radius:4,initial_threshold_m:0.2,slope:0.1,max_threshold_m:1.,max_work_bytes:8192,max_cell_passes:10000}}
 #[test]fn removes_three_by_three_roof_preserves_flat_ground(){
 let mut z=vec![10.;81];for y in 3..6{for x in 3..6{z[y*9+x]=15.;}}
 let q=filter(g(),&z,p(),||false).unwrap();assert_eq!(q.rejected,9);assert_eq!(q.accepted,72);assert!(q.elevations[40].is_nan());}
 #[test]fn nodata_is_not_filled_and_budget_checked(){
 let mut z=vec![f32::NAN;81];z[40]=10.;let q=filter(g(),&z,p(),||false).unwrap();assert_eq!(q.unsupported,1);assert_eq!(q.accepted,0);
 assert!(matches!(filter(g(),&z,Policy{max_work_bytes:1,..p()},||false),Err(Error::Limit)));}
}
