//! Global additive radiometric offsets from trusted aligned overlap statistics.
//! Anchored weighted least squares on the overlap graph, bounded coordinate
//! relaxation, no dense matrix. Not a spatial seamline or exposure gain solver.
use std::collections::{BTreeMap,BTreeSet,VecDeque};
#[derive(Debug,Clone,Copy)]pub struct Edge{pub a:u32,pub b:u32,pub offset:[i16;3],pub samples:u32}
#[derive(Debug,Clone,Copy)]pub struct Policy{pub min_samples:u32,pub max_offset:f64,pub max_residual:f64,pub tolerance:f64,pub iterations:u32}
#[derive(Debug)]pub struct Report{pub offsets:BTreeMap<u32,[f64;3]>,pub max_residual:f64,pub iterations:u32}
#[derive(Debug,Clone,Copy,PartialEq,Eq)]pub enum Error{Invalid,Limit,Disconnected,NonConverged,Residual,Cancelled}
/// Edge offset is the correction to B relative to A: offset_b - offset_a.
pub fn solve(ids:&[u32],edges:&[Edge],anchor:u32,p:Policy,mut cancelled:impl FnMut()->bool)->Result<Report,Error>{
 if ids.len()>100_000||edges.len()>1_000_000||p.iterations>2000{return Err(Error::Limit);}
 if ids.is_empty()||p.min_samples==0||p.iterations==0||
 !p.max_offset.is_finite()||p.max_offset<=0.||p.max_offset>64.||
 !p.max_residual.is_finite()||p.max_residual<=0.||!p.tolerance.is_finite()||p.tolerance<=0.{return Err(Error::Invalid);}
 let mut index=BTreeMap::new();
 for &id in ids{if index.insert(id,0usize).is_some(){return Err(Error::Invalid);}}
 for(i,v)in index.values_mut().enumerate(){*v=i;}
 let root=*index.get(&anchor).ok_or(Error::Invalid)?;
 let mut adj=vec![Vec::<(usize,[f64;3],f64)>::new();ids.len()];let mut seen=BTreeSet::new();
 for e in edges{
 if cancelled(){return Err(Error::Cancelled);}
 let a=*index.get(&e.a).ok_or(Error::Invalid)?;let b=*index.get(&e.b).ok_or(Error::Invalid)?;
 if a==b||!seen.insert((a.min(b),a.max(b)))||e.offset.iter().any(|v|v.unsigned_abs()>64){return Err(Error::Invalid);}
 if e.samples<p.min_samples{continue;}
 let d=e.offset.map(f64::from);let w=f64::from(e.samples.min(1_000_000));
 adj[a].push((b,d.map(|x|-x),w));adj[b].push((a,d,w));
 }
 let mut x=vec![[0.;3];ids.len()];let mut visited=vec![false;ids.len()];visited[root]=true;
 let mut q=VecDeque::from([root]);
 while let Some(i)=q.pop_front(){
 for &(j,d,_)in &adj[i]{if !visited[j]{visited[j]=true;
 x[j]=std::array::from_fn(|k|(x[i][k]-d[k]).clamp(-p.max_offset,p.max_offset));q.push_back(j);}}
 }
 if visited.iter().any(|v|!*v){return Err(Error::Disconnected);}
 let mut iterations=0;
 for _ in 0..p.iterations{
 let mut change=0.0f64;
 for i in 0..x.len(){
 if i%256==0&&cancelled(){return Err(Error::Cancelled);}
 if i==root{continue;}
 let mut next=[0.;3];let mut sum=0.;
 for &(j,d,w)in &adj[i]{sum+=w;for k in 0..3{next[k]+=w*(x[j][k]+d[k]);}}
 if sum<=0.{return Err(Error::Disconnected);}
 for k in 0..3{let a=(next[k]/sum).clamp(-p.max_offset,p.max_offset);change=change.max((a-x[i][k]).abs());x[i][k]=a;}
 }
 iterations+=1;
 if change<=p.tolerance{break;}
 if iterations==p.iterations{return Err(Error::NonConverged);}
 }
 let mut max_residual=0.0f64;
 for (i,links)in adj.iter().enumerate(){if i%256==0&&cancelled(){return Err(Error::Cancelled);}
 for &(j,d,_)in links{for k in 0..3{max_residual=max_residual.max((x[i][k]-x[j][k]-d[k]).abs());}}}
 if max_residual>p.max_residual{return Err(Error::Residual);}
 Ok(Report{offsets:index.into_iter().map(|(id,i)|(id,x[i])).collect(),max_residual,iterations})
}
#[cfg(test)]mod tests{
 use super::*;
 fn p()->Policy{Policy{min_samples:10,max_offset:64.,max_residual:0.1,tolerance:1e-8,iterations:100}}
 fn e(a:u32,b:u32,d:i16)->Edge{Edge{a,b,offset:[d;3],samples:100}}
 #[test]fn anchored_loop(){let r=solve(&[3,1,2],&[e(1,2,10),e(2,3,5),e(1,3,15)],1,p(),||false).unwrap();
 assert_eq!(r.offsets[&1],[0.;3]);assert!((r.offsets[&3][0]-15.).abs()<1e-7);}
 #[test]fn rejects_disconnected_or_inconsistent(){assert!(matches!(solve(&[1,2,3],&[e(1,2,10)],1,p(),||false),Err(Error::Disconnected)));
 assert!(matches!(solve(&[1,2,3],&[e(1,2,10),e(2,3,10),e(1,3,40)],1,p(),||false),Err(Error::Residual)));}
}
