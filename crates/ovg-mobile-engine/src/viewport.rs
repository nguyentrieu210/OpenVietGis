//! Revision-scoped bounded cache. Native renderer may omit display objects,
 //! but must never mutate the authoritative store.
use std::collections::{HashMap,VecDeque};
use std::sync::Arc;
use crate::Error;
#[derive(Clone,Debug,Eq,PartialEq,Hash)]
pub struct Key {pub workspace:String,pub layer:String,pub revision:u64,pub style:u64,pub zoom:u8,pub x:u32,pub y:u32}
pub struct Cache {budget:usize,used:usize,epoch:u64,entries:HashMap<Key,Arc<[u8]>>,order:VecDeque<Key>}
impl Cache {
    pub fn new(budget:usize)->Result<Self,Error>{if budget==0||budget>192*1024*1024{return Err(Error::Limit);}Ok(Self{budget,used:0,epoch:0,entries:HashMap::new(),order:VecDeque::new()})}
    pub fn next_epoch(&mut self)->Result<u64,Error>{self.epoch=self.epoch.checked_add(1).ok_or(Error::Limit)?;Ok(self.epoch)}
    pub fn insert(&mut self,epoch:u64,key:Key,data:Arc<[u8]>)->Result<(),Error>{
        if epoch!=self.epoch{return Err(Error::Stale);}
        if data.len()>self.budget||data.len()>crate::MAX_BYTES||key.workspace.len()>128||key.layer.len()>128||key.zoom>24{return Err(Error::Limit);}
        self.remove(&key);
        while self.used+data.len()>self.budget||self.entries.len()>=4096{let k=self.order.pop_front().ok_or(Error::Limit)?;self.remove(&k);}
        self.used+=data.len();self.order.push_back(key.clone());self.entries.insert(key,data);Ok(())
    }
    pub fn get(&mut self,key:&Key)->Option<Arc<[u8]>>{let v=self.entries.get(key)?.clone();self.order.retain(|k|k!=key);self.order.push_back(key.clone());Some(v)}
    fn remove(&mut self,key:&Key){if let Some(v)=self.entries.remove(key){self.used-=v.len();}self.order.retain(|k|k!=key);}
    pub fn trim(&mut self,budget:usize){self.budget=budget.max(1).min(192*1024*1024);while self.used>self.budget{if let Some(k)=self.order.pop_front(){self.remove(&k);}else{break;}}}
    pub fn invalidate_layer(&mut self,workspace:&str,layer:&str){let keys:Vec<_>=self.entries.keys().filter(|k|k.workspace==workspace&&k.layer==layer).cloned().collect();for k in keys{self.remove(&k);}}
    pub fn bytes(&self)->usize{self.used}
}
