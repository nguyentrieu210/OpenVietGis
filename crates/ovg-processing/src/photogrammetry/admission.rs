//! Atomic shared host / scratch / output / per-device admission accounting.
//! Synchronize access in the runtime; actual OS resource isolation is separate.
use std::collections::BTreeMap;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Host { pub bytes:u64, pub disk:u64, pub threads:u32 }
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DeviceLimit { pub bytes:u64, pub unified:bool }
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Demand { pub host_bytes:u64, pub scratch:u64, pub output:u64, pub threads:u32,
    pub gpu:Option<(String,u64)> }
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Error { Invalid, Duplicate, UnknownDevice, Exhausted, Overflow }
pub struct Pool {
    cap:Host, used:Host, devices:BTreeMap<String,DeviceLimit>, gpu_used:BTreeMap<String,u64>,
    active:BTreeMap<String,Demand>,
}
fn add(a:u64,b:u64)->Result<u64,Error>{a.checked_add(b).ok_or(Error::Overflow)}
impl Pool {
    pub fn new(cap:Host,devices:BTreeMap<String,DeviceLimit>)->Self{
        Self{cap,used:Host::default(),devices,gpu_used:BTreeMap::new(),active:BTreeMap::new()}
    }
    pub fn used(&self)->Host{self.used}
    pub fn device_used(&self,id:&str)->u64{self.gpu_used.get(id).copied().unwrap_or(0)}
    fn charge(&self,d:&Demand)->Result<Host,Error>{
        let mut host=d.host_bytes;
        if let Some((id,bytes))=&d.gpu{
            if *bytes==0 {return Err(Error::Invalid);}
            if self.devices.get(id).ok_or(Error::UnknownDevice)?.unified {host=add(host,*bytes)?;}
        }
        Ok(Host{bytes:host,disk:add(d.scratch,d.output)?,threads:d.threads})
    }
    pub fn reserve(&mut self,id:&str,d:Demand)->Result<(),Error>{
        if !super::valid_id(id)||d.host_bytes==0||d.threads==0 {return Err(Error::Invalid);}
        if self.active.contains_key(id){return Err(Error::Duplicate);}
        let h=self.charge(&d)?;
        let next=Host{bytes:add(self.used.bytes,h.bytes)?,disk:add(self.used.disk,h.disk)?,
            threads:self.used.threads.checked_add(h.threads).ok_or(Error::Overflow)?};
        if next.bytes>self.cap.bytes||next.disk>self.cap.disk||next.threads>self.cap.threads{
            return Err(Error::Exhausted);
        }
        let gpu=if let Some((gpu,bytes))=&d.gpu {
            let cap=self.devices.get(gpu).ok_or(Error::UnknownDevice)?;
            let n=add(self.device_used(gpu),*bytes)?;
            if n>cap.bytes{return Err(Error::Exhausted);}
            Some((gpu.clone(),n))
        }else{None};
        if let Some((gpu,n))=gpu{self.gpu_used.insert(gpu,n);}
        self.used=next;self.active.insert(id.into(),d);Ok(())
    }
    pub fn release(&mut self,id:&str)->bool{
        let Some(d)=self.active.get(id)else{return false;};
        // All values and device identities were validated on insertion.
        let Ok(h)=self.charge(d)else{return false;};
        let d=self.active.remove(id).expect("active demand");
        self.used.bytes-=h.bytes;self.used.disk-=h.disk;self.used.threads-=h.threads;
        if let Some((gpu,n))=d.gpu {
            let next=self.device_used(&gpu)-n;
            if next==0{self.gpu_used.remove(&gpu);}else{self.gpu_used.insert(gpu,next);}
        }
        true
    }
}
#[cfg(test)]
mod tests{
    use super::*;
    fn pool()->Pool{Pool::new(Host{bytes:100,disk:100,threads:8},[
        ("g0".into(),DeviceLimit{bytes:30,unified:false}),
        ("g1".into(),DeviceLimit{bytes:30,unified:true})].into_iter().collect())}
    fn demand(gpu:&str)->Demand{Demand{host_bytes:30,scratch:15,output:10,threads:2,gpu:Some((gpu.into(),20))}}
    #[test] fn per_device_and_shared_host_are_atomic(){
        let mut p=pool();p.reserve("a",demand("g0")).unwrap();
        assert_eq!(p.reserve("b",demand("g0")),Err(Error::Exhausted));
        assert_eq!(p.used().bytes,30);assert_eq!(p.device_used("g0"),20);
        p.reserve("b",demand("g1")).unwrap();assert_eq!(p.used().bytes,80);
        assert!(p.release("a"));assert!(p.release("b"));assert_eq!(p.used(),Host::default());
    }
    #[test] fn output_bytes_and_scratch_share_disk_budget(){
        let mut p=pool();let mut d=demand("g0");d.gpu=None;d.output=90;
        assert_eq!(p.reserve("a",d),Err(Error::Exhausted));assert_eq!(p.used(),Host::default());
    }
    #[test] fn unknown_device_and_duplicate_refused(){
        let mut p=pool();assert_eq!(p.reserve("a",demand("g9")),Err(Error::UnknownDevice));
        p.reserve("a",demand("g0")).unwrap();
        assert_eq!(p.reserve("a",demand("g1")),Err(Error::Duplicate));
        assert_eq!(p.device_used("g1"),0);
    }
}
