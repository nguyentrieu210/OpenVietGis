//! In-memory job state machine. Durability and OS worker termination remain host obligations.
use super::dag::{Graph,Node,Error as GraphError};
use std::collections::{BTreeMap,BTreeSet};
#[derive(Debug,Clone,Copy,PartialEq,Eq)]
pub enum State {Queued,Running,RetryWait,Succeeded,Failed,Cancelled}
#[derive(Debug,Clone,PartialEq,Eq)]
pub struct Status {pub state:State,pub attempt:u8,pub lease:Option<String>,pub deadline_ms:u64}
#[derive(Debug,Clone,Copy,PartialEq,Eq)]
pub enum Error {Graph(GraphError),Invalid,NotReady,StaleLease,Expired}
pub struct Scheduler {graph:Graph,deps:BTreeMap<String,Vec<String>>,states:BTreeMap<String,Status>,leases:BTreeSet<String>,max_attempts:u8}
impl Scheduler{
    pub fn new(nodes:Vec<Node>,max_attempts:u8)->Result<Self,Error>{
        if max_attempts==0||max_attempts>3{return Err(Error::Invalid);}
        let graph=Graph::new(nodes.clone(),100_000,1_000_000).map_err(Error::Graph)?;
        let deps=nodes.into_iter().map(|n|(n.id,n.dependencies)).collect();
        let states=graph.order().iter().map(|id|(id.clone(),Status{state:State::Queued,attempt:0,lease:None,deadline_ms:0})).collect();
        Ok(Self{graph,deps,states,leases:BTreeSet::new(),max_attempts})
    }
    pub fn status(&self,id:&str)->Option<&Status>{self.states.get(id)}
    pub fn ready(&self)->Vec<String>{
        self.graph.order().iter().filter(|id|{
            let s=&self.states[*id];
            matches!(s.state,State::Queued|State::RetryWait)&&self.deps[*id].iter().all(|d|self.states[d].state==State::Succeeded)
        }).cloned().collect()
    }
    pub fn start(&mut self,id:&str,lease:&str,now:u64,deadline:u64)->Result<(),Error>{
        if !super::valid_id(lease)||deadline<=now||self.leases.contains(lease)||self.leases.len()>=300_000{return Err(Error::Invalid);}
        if !self.ready().iter().any(|n|n==id){return Err(Error::NotReady);}
        let s=self.states.get_mut(id).ok_or(Error::Invalid)?;
        if s.attempt>=self.max_attempts{return Err(Error::NotReady);}
        s.attempt+=1;s.state=State::Running;s.lease=Some(lease.into());s.deadline_ms=deadline;
        self.leases.insert(lease.into());Ok(())
    }
    /// Caller publishes verified artifacts transactionally before reporting success.
    pub fn finish(&mut self,id:&str,lease:&str,now:u64,success:bool,transient:bool)->Result<(),Error>{
        let s=self.states.get_mut(id).ok_or(Error::Invalid)?;
        if s.state!=State::Running||s.lease.as_deref()!=Some(lease){return Err(Error::StaleLease);}
        if now>=s.deadline_ms{return Err(Error::Expired);}
        s.state=if success{State::Succeeded}else if transient&&s.attempt<self.max_attempts{State::RetryWait}else{State::Failed};
        s.lease=None;s.deadline_ms=0;Ok(())
    }
    pub fn expire(&mut self,now:u64)->Vec<String>{
        let mut ids=Vec::new();
        for(id,s)in &mut self.states{
            if s.state==State::Running&&now>=s.deadline_ms{
                ids.push(id.clone());s.state=if s.attempt<self.max_attempts{State::RetryWait}else{State::Failed};
                s.lease=None;s.deadline_ms=0;
            }
        }
        ids
    }
    pub fn cancel_all(&mut self){
        for s in self.states.values_mut(){
            if matches!(s.state,State::Queued|State::Running|State::RetryWait){s.state=State::Cancelled;s.lease=None;s.deadline_ms=0;}
        }
    }
}
#[cfg(test)]
mod tests{
    use super::*;
    fn sched()->Scheduler{Scheduler::new(vec![Node{id:"a".into(),dependencies:vec![]},Node{id:"b".into(),dependencies:vec!["a".into()]}],2).unwrap()}
    #[test]fn dependencies_and_stale_receipts(){
        let mut s=sched();assert_eq!(s.ready(),vec!["a"]);
        assert_eq!(s.start("b","x",0,10),Err(Error::NotReady));
        s.start("a","l1",0,10).unwrap();
        assert_eq!(s.finish("a","l0",1,true,false),Err(Error::StaleLease));
        s.finish("a","l1",1,true,false).unwrap();assert_eq!(s.ready(),vec!["b"]);
    }
    #[test]fn retry_and_expiry_are_bounded(){
        let mut s=sched();s.start("a","l1",0,10).unwrap();
        assert_eq!(s.finish("a","l1",10,true,false),Err(Error::Expired));
        assert_eq!(s.expire(10),vec!["a"]);s.start("a","l2",10,20).unwrap();
        s.finish("a","l2",11,false,true).unwrap();assert_eq!(s.status("a").unwrap().state,State::Failed);
        assert!(s.ready().is_empty());
    }
    #[test]fn cancellation_refuses_late_success(){
        let mut s=sched();s.start("a","l1",0,10).unwrap();s.cancel_all();
        assert_eq!(s.finish("a","l1",1,true,false),Err(Error::StaleLease));
    }
}
