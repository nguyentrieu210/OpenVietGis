//! Spatial candidate generation on one validated metric project grid.
//! Bounding-box overlap is a candidate heuristic, not verified feature matching.
use std::collections::{BTreeMap,BTreeSet};
#[derive(Debug,Clone,Copy)]
pub struct Footprint {pub id:u32,pub min:[f64;2],pub max:[f64;2]}
#[derive(Debug,Clone,Copy)]
pub struct Policy {pub cell_size:f64,pub min_overlap:f64,pub max_cells_per_image:usize,pub max_bucket:usize,pub max_pair_checks:usize,pub max_pairs:usize}
#[derive(Debug,Clone,Copy,PartialEq,Eq)]
pub enum Error {Invalid,Duplicate,Limit}
fn cell(v:f64,size:f64)->Result<i64,Error>{
    let n=(v/size).floor();
    if !n.is_finite()||!(-1e12..=1e12).contains(&n){return Err(Error::Invalid);}
    Ok(n as i64)
}
fn overlap(a:Footprint,b:Footprint)->f64{
    let w=(a.max[0].min(b.max[0])-a.min[0].max(b.min[0])).max(0.0);
    let h=(a.max[1].min(b.max[1])-a.min[1].max(b.min[1])).max(0.0);
    let area=|f:Footprint|(f.max[0]-f.min[0])*(f.max[1]-f.min[1]);
    w*h/area(a).min(area(b))
}
pub fn candidates(images:&[Footprint],p:Policy)->Result<Vec<(u32,u32)>,Error>{
    if images.len()>100_000||!p.cell_size.is_finite()||p.cell_size<=0.0||!p.min_overlap.is_finite()||
        p.min_overlap<=0.0||p.min_overlap>1.0||p.max_cells_per_image==0||p.max_cells_per_image>4096||
        p.max_bucket==0||p.max_bucket>100_000||p.max_pair_checks>20_000_000||
        p.max_pair_checks==0||p.max_pairs==0||p.max_pairs>5_000_000{return Err(Error::Invalid);}
    let mut grid:BTreeMap<(i64,i64),Vec<Footprint>>=BTreeMap::new();
    let mut ids=BTreeSet::new();let mut pairs=BTreeSet::new();let mut checks=0usize;let mut entries=0usize;
    for &f in images{
        if !ids.insert(f.id){return Err(Error::Duplicate);}
        if !(0..2).all(|i|f.min[i].is_finite()&&f.max[i].is_finite()&&f.max[i]>f.min[i]){
            return Err(Error::Invalid);}
        let area=(f.max[0]-f.min[0])*(f.max[1]-f.min[1]);
        if !area.is_finite()||area<=0.0{return Err(Error::Invalid);}
        let x0=cell(f.min[0],p.cell_size)?;let x1=cell(f.max[0],p.cell_size)?;
        let y0=cell(f.min[1],p.cell_size)?;let y1=cell(f.max[1],p.cell_size)?;
        let count=(x1-x0+1).checked_mul(y1-y0+1).ok_or(Error::Limit)?;
        if count>p.max_cells_per_image as i64{return Err(Error::Limit);}
        entries=entries.checked_add(count as usize).ok_or(Error::Limit)?;
        if entries>2_000_000{return Err(Error::Limit);}
        for x in x0..=x1{for y in y0..=y1{
            let bucket=grid.entry((x,y)).or_default();
            if bucket.len()>=p.max_bucket{return Err(Error::Limit);}
            for &other in bucket.iter(){
                checks+=1;if checks>p.max_pair_checks{return Err(Error::Limit);}
                if overlap(f,other)>=p.min_overlap{
                    pairs.insert((f.id.min(other.id),f.id.max(other.id)));
                    if pairs.len()>p.max_pairs{return Err(Error::Limit);}
                }
            }
            bucket.push(f);
        }}
    }
    Ok(pairs.into_iter().collect())
}
#[cfg(test)]mod tests{
    use super::*;
    fn p()->Policy{Policy{cell_size:10.,min_overlap:0.2,max_cells_per_image:64,max_bucket:32,max_pair_checks:1000,max_pairs:100}}
    fn f(id:u32,x:f64)->Footprint{Footprint{id,min:[x,0.],max:[x+10.,10.]}}
    #[test]fn overlap_only_and_unique(){assert_eq!(candidates(&[f(3,5.),f(1,0.),f(2,100.)],p()).unwrap(),vec![(1,3)]);}
    #[test]fn density_and_invalid_fail_closed(){let mut q=p();q.max_bucket=1;assert_eq!(candidates(&[f(1,0.),f(2,1.)],q),Err(Error::Limit));
        assert_eq!(candidates(&[f(1,0.),f(1,1.)],p()),Err(Error::Duplicate));}
}
