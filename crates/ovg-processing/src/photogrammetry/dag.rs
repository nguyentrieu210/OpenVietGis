//! Deterministic bounded DAG and dependency-aware invalidation.
use std::collections::{BTreeMap,BTreeSet,VecDeque};
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Node { pub id:String,pub dependencies:Vec<String> }
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Error { Limit, InvalidId, Duplicate, MissingDependency, Cycle }
pub struct Graph { nodes:BTreeMap<String,Vec<String>>, reverse:BTreeMap<String,BTreeSet<String>>, order:Vec<String> }
impl Graph {
    pub fn new(nodes:Vec<Node>,max_nodes:usize,max_edges:usize)->Result<Self,Error>{
        if nodes.len()>max_nodes||max_nodes>1_000_000||max_edges>10_000_000{return Err(Error::Limit);}
        let mut map=BTreeMap::new();let mut edges=0usize;
        for n in nodes{
            if !super::valid_id(&n.id){return Err(Error::InvalidId);}
            edges=edges.checked_add(n.dependencies.len()).ok_or(Error::Limit)?;
            if edges>max_edges{return Err(Error::Limit);}
            let unique:BTreeSet<_>=n.dependencies.iter().collect();
            if unique.len()!=n.dependencies.len(){return Err(Error::Duplicate);}
            if map.insert(n.id,n.dependencies).is_some(){return Err(Error::Duplicate);}
        }
        let mut reverse:BTreeMap<String,BTreeSet<String>>=BTreeMap::new();
        let mut counts=BTreeMap::new();let mut ready=BTreeSet::new();
        for (id,deps) in &map{
            counts.insert(id.clone(),deps.len());
            if deps.is_empty(){ready.insert(id.clone());}
            for d in deps{if !map.contains_key(d){return Err(Error::MissingDependency);}
                reverse.entry(d.clone()).or_default().insert(id.clone());}
        }
        let mut order=Vec::with_capacity(map.len());
        while let Some(id)=ready.pop_first(){
            if let Some(children)=reverse.get(&id){for child in children{
                let n=counts.get_mut(child).expect("validated child");*n-=1;if *n==0{ready.insert(child.clone());}
            }}
            order.push(id);
        }
        if order.len()!=map.len(){return Err(Error::Cycle);}
        Ok(Self{nodes:map,reverse,order})
    }
    pub fn order(&self)->&[String]{&self.order}
    /// Return changed nodes and all transitive consumers in deterministic order.
    pub fn invalidated(&self,changed:&[String])->Result<Vec<String>,Error>{
        let mut seen=BTreeSet::new();let mut q=VecDeque::new();
        for id in changed{if !self.nodes.contains_key(id){return Err(Error::MissingDependency);}
            if seen.insert(id.clone()){q.push_back(id.clone());}}
        while let Some(id)=q.pop_front(){if let Some(children)=self.reverse.get(&id){
            for child in children{if seen.insert(child.clone()){q.push_back(child.clone());}}
        }}
        Ok(self.order.iter().filter(|n|seen.contains(*n)).cloned().collect())
    }
}
#[cfg(test)]
mod tests{
    use super::*;
    fn n(id:&str,deps:&[&str])->Node{Node{id:id.into(),dependencies:deps.iter().map(|s|s.to_string()).collect()}}
    #[test] fn deterministic_dependency_order_and_dirty_closure(){
        let g=Graph::new(vec![n("export",&["tile"]),n("tile",&["camera"]),n("camera",&[]),n("other",&[])],10,10).unwrap();
        assert_eq!(g.order(),&["camera","other","tile","export"]);
        assert_eq!(g.invalidated(&["camera".into()]).unwrap(),vec!["camera","tile","export"]);
    }
    #[test] fn rejects_cycles_missing_and_duplicates(){
        assert!(matches!(Graph::new(vec![n("a",&["b"]),n("b",&["a"])],10,10),Err(Error::Cycle)));
        assert!(matches!(Graph::new(vec![n("a",&["b"])],10,10),Err(Error::MissingDependency)));
        assert!(matches!(Graph::new(vec![n("a",&[]),n("a",&[])],10,10),Err(Error::Duplicate)));
    }
}
