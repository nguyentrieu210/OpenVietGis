//! In-memory immutable artifact-reference revisions with optimistic publication.
//! The trusted host verifies file bytes and persists a transaction before exposing a revision.
use std::collections::{BTreeMap,BTreeSet};
#[derive(Debug,Clone,PartialEq,Eq)]
pub struct Artifact {pub sha256:String,pub bytes:u64,pub dependencies:Vec<String>}
#[derive(Debug,Clone,PartialEq,Eq)]
pub struct Revision {pub id:u64,pub parent:Option<u64>,pub tiles:BTreeMap<String,Artifact>}
#[derive(Debug,Clone,Copy,PartialEq,Eq)]
pub enum Error {Conflict,Invalid,Limit,Overflow,MissingRevision}
pub struct Store{head:u64,revisions:BTreeMap<u64,Revision>,max_tiles:usize,max_revisions:usize}
impl Store{
    pub fn new(max_tiles:usize,max_revisions:usize)->Result<Self,Error>{
        if max_tiles==0||max_tiles>1_000_000||max_revisions<2||max_revisions>100_000{return Err(Error::Limit);}
        let genesis=Revision{id:0,parent:None,tiles:BTreeMap::new()};
        Ok(Self{head:0,revisions:[(0,genesis)].into_iter().collect(),max_tiles,max_revisions})
    }
    pub fn head(&self)->&Revision{&self.revisions[&self.head]}
    pub fn revision(&self,id:u64)->Option<&Revision>{self.revisions.get(&id)}
    /// None deletes a tile from the new revision only; past revisions remain readable.
    pub fn publish(&mut self,expected:u64,changes:BTreeMap<String,Option<Artifact>>)->Result<u64,Error>{
        if expected!=self.head{return Err(Error::Conflict);}
        if changes.is_empty(){return Err(Error::Invalid);}
        if self.revisions.len()>=self.max_revisions||changes.len()>self.max_tiles{return Err(Error::Limit);}
        let mut tiles=self.head().tiles.clone();
        for(id,change)in changes{
            if !super::valid_id(&id){return Err(Error::Invalid);}
            if let Some(a)=change{
                if !super::digest(&a.sha256)||a.bytes==0||a.dependencies.len()>super::MAX_DEPENDENCIES||
                    a.dependencies.iter().any(|d|!super::digest(d)){return Err(Error::Invalid);}
                tiles.insert(id,a);
            }else{tiles.remove(&id);}
            if tiles.len()>self.max_tiles{return Err(Error::Limit);}
        }
        let id=self.head.checked_add(1).ok_or(Error::Overflow)?;
        self.revisions.insert(id,Revision{id,parent:Some(self.head),tiles});self.head=id;Ok(id)
    }
    /// Retained hashes include dependency references; leases must be added by host before GC.
    pub fn retained_hashes(&self)->BTreeSet<String>{
        let mut out=BTreeSet::new();
        for r in self.revisions.values(){for a in r.tiles.values(){
            out.insert(a.sha256.clone());out.extend(a.dependencies.iter().cloned());
        }}
        out
    }
}
#[cfg(test)]
mod tests{
    use super::*;
    fn a(c:char)->Artifact{Artifact{sha256:c.to_string().repeat(64),bytes:10,dependencies:vec!["f".repeat(64)]}}
    #[test]fn old_revisions_immutable_and_cas(){
        let mut s=Store::new(10,10).unwrap();
        s.publish(0,[("tile".into(),Some(a('a')))].into_iter().collect()).unwrap();
        s.publish(1,[("tile".into(),Some(a('b')))].into_iter().collect()).unwrap();
        assert_eq!(s.revision(1).unwrap().tiles["tile"],a('a'));
        assert_eq!(s.publish(1,[("tile".into(),None)].into_iter().collect()),Err(Error::Conflict));
        assert_eq!(s.retained_hashes().len(),3);
    }
    #[test]fn invalid_change_keeps_head(){
        let mut s=Store::new(1,10).unwrap();
        assert_eq!(s.publish(0,[("a".into(),Some(a('x')))].into_iter().collect()),Err(Error::Invalid));assert_eq!(s.head().id,0);
    }
}
