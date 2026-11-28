//! Byte-bounded LRU cache for immutable, host-verified source revisions.
//! Shared buffers avoid copying decoded pixels into each tile's view list.
use std::{collections::{BTreeMap,VecDeque},path::{Path,PathBuf},sync::Arc};
pub struct Cache{max:u64,bytes:u64,items:BTreeMap<PathBuf,Arc<[u8]>>,lru:VecDeque<PathBuf>}
impl Cache{
 pub fn new(max:u64)->Self{Self{max,bytes:0,items:BTreeMap::new(),lru:VecDeque::new()}}
 pub fn bytes(&self)->u64{self.bytes}
 pub fn take(&mut self,key:&Path)->Option<Arc<[u8]>>{
  let value=self.items.remove(key)?;self.bytes-=value.len() as u64;
  self.lru.retain(|k|k!=key);Some(value)
 }
 pub fn trim_to(&mut self,limit:u64){
  let limit=limit.min(self.max);
  while self.bytes>limit{
   let Some(key)=self.lru.pop_front()else{break;};
   if let Some(value)=self.items.remove(&key){self.bytes-=value.len() as u64;}
  }
 }
 pub fn insert(&mut self,key:PathBuf,value:Arc<[u8]>){
  self.take(&key);let len=value.len() as u64;
  if len==0||len>self.max{return;}
  self.trim_to(self.max-len);
  // Bound metadata overhead, independently from decoded-byte budget.
  while self.items.len()>=4096{
   if let Some(old)=self.lru.pop_front(){if let Some(v)=self.items.remove(&old){self.bytes-=v.len() as u64;}}
   else{break;}
  }
  self.bytes+=len;self.lru.push_back(key.clone());self.items.insert(key,value);
 }
}
#[cfg(test)]mod tests{
 use super::*;
 #[test]fn bounded_lru_and_shared_buffer(){
 let mut c=Cache::new(8);let a:Arc<[u8]>=Arc::from([1u8;4]);
 c.insert("a".into(),a.clone());c.insert("b".into(),Arc::from([2u8;4]));
 let v=c.take(Path::new("a")).unwrap();assert!(Arc::ptr_eq(&a,&v));c.insert("a".into(),v);
 c.insert("c".into(),Arc::from([3u8;4]));assert!(c.take(Path::new("b")).is_none());assert_eq!(c.bytes(),8);
 c.trim_to(0);assert_eq!(c.bytes(),0);
 }
 #[test]fn rejects_oversize_and_zero_capacity(){
 let mut c=Cache::new(2);c.insert("a".into(),Arc::from([0u8;3]));assert_eq!(c.bytes(),0);
 }
}
