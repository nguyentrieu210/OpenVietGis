//! Verify camera-match graph components before global reconstruction.
//! Inlier counts and angles must originate from a qualified geometry provider.
use std::collections::{BTreeMap,BTreeSet,VecDeque};
#[derive(Debug,Clone,Copy)]
pub struct Edge {pub a:u32,pub b:u32,pub inliers:u32,pub median_angle_deg:f64}
#[derive(Debug,Clone,Copy,PartialEq,Eq)]
pub enum Error{Invalid,Duplicate,Limit}
pub fn components(cameras:&[u32],edges:&[Edge],min_inliers:u32,min_angle:f64)->Result<Vec<Vec<u32>>,Error>{
    if cameras.len()>100_000||edges.len()>5_000_000{return Err(Error::Limit);}
    if cameras.is_empty()||min_inliers==0||!min_angle.is_finite()||!(0.0..=180.0).contains(&min_angle){return Err(Error::Invalid);}
    let mut graph:BTreeMap<u32,Vec<u32>>=BTreeMap::new();
    for &id in cameras{if graph.insert(id,Vec::new()).is_some(){return Err(Error::Duplicate);}}
    let mut pairs=BTreeSet::new();
    for e in edges{
        if e.a==e.b||!graph.contains_key(&e.a)||!graph.contains_key(&e.b)||
            !e.median_angle_deg.is_finite()||!(0.0..=180.0).contains(&e.median_angle_deg){return Err(Error::Invalid);}
        if !pairs.insert((e.a.min(e.b),e.a.max(e.b))){return Err(Error::Duplicate);}
        if e.inliers>=min_inliers&&e.median_angle_deg>=min_angle{
            graph.get_mut(&e.a).unwrap().push(e.b);graph.get_mut(&e.b).unwrap().push(e.a);
        }
    }
    let mut seen=BTreeSet::new();let mut groups=Vec::new();
    for &id in graph.keys(){
        if !seen.insert(id){continue;}
        let mut q=VecDeque::from([id]);let mut group=Vec::new();
        while let Some(n)=q.pop_front(){group.push(n);for &child in &graph[&n]{
            if seen.insert(child){q.push_back(child);}
        }}
        group.sort_unstable();groups.push(group);
    }
    groups.sort_by(|a,b|b.len().cmp(&a.len()).then_with(||a.cmp(b)));Ok(groups)
}
#[cfg(test)]mod tests{
    use super::*;
    #[test]fn weak_edges_do_not_join_clusters(){
        let e=[Edge{a:1,b:2,inliers:50,median_angle_deg:5.},Edge{a:2,b:3,inliers:3,median_angle_deg:5.}];
        assert_eq!(components(&[4,3,2,1],&e,10,1.).unwrap(),vec![vec![1,2],vec![3],vec![4]]);
    }
    #[test]fn invalid_camera_fails(){assert_eq!(components(&[1],&[Edge{a:1,b:2,inliers:10,median_angle_deg:1.}],1,0.),Err(Error::Invalid));}
}
