//! Resource admission accounting only. Does not replace OS containment.
use super::Budget;
use std::collections::BTreeMap;
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AdmissionError { Invalid, Duplicate, Exhausted, Overflow }
pub struct Governor {
    limit: Budget, used: Budget, active: BTreeMap<String,Budget>,
}
impl Governor {
    pub fn new(limit:Budget)->Self {
        Self{limit,used:Budget{host_bytes:0,device_bytes:0,scratch_bytes:0,threads:0},active:BTreeMap::new()}
    }
    pub fn used(&self)->Budget {self.used}
    /// Call under the host's shared lock. One governor per physical GPU and a
    /// shared host governor must be composed atomically by the scheduler.
    pub fn reserve(&mut self,id:&str,b:Budget)->Result<(),AdmissionError> {
        if id.is_empty() || id.len()>128 || b.host_bytes==0 || b.threads==0 {return Err(AdmissionError::Invalid);}
        if self.active.contains_key(id){return Err(AdmissionError::Duplicate);}
        let next=Budget{
            host_bytes:self.used.host_bytes.checked_add(b.host_bytes).ok_or(AdmissionError::Overflow)?,
            device_bytes:self.used.device_bytes.checked_add(b.device_bytes).ok_or(AdmissionError::Overflow)?,
            scratch_bytes:self.used.scratch_bytes.checked_add(b.scratch_bytes).ok_or(AdmissionError::Overflow)?,
            threads:self.used.threads.checked_add(b.threads).ok_or(AdmissionError::Overflow)?,
        };
        if !next.fits(self.limit){return Err(AdmissionError::Exhausted);}
        self.active.insert(id.into(),b);self.used=next;Ok(())
    }
    pub fn release(&mut self,id:&str)->bool {
        if let Some(b)=self.active.remove(id) {
            self.used.host_bytes-=b.host_bytes;self.used.device_bytes-=b.device_bytes;
            self.used.scratch_bytes-=b.scratch_bytes;self.used.threads-=b.threads;true
        } else {false}
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn b()->Budget {Budget{host_bytes:100,device_bytes:10,scratch_bytes:50,threads:2}}
    #[test] fn admission_is_atomic_and_reusable(){
        let mut g=Governor::new(b());g.reserve("a",b()).unwrap();
        assert_eq!(g.reserve("a",b()),Err(AdmissionError::Duplicate));
        assert_eq!(g.reserve("b",b()),Err(AdmissionError::Exhausted));
        assert_eq!(g.used(),b());assert!(g.release("a"));assert!(!g.release("a"));
        g.reserve("b",b()).unwrap();
    }
    #[test] fn overflow_does_not_mutate(){
        let mut limit=b();limit.host_bytes=u64::MAX;limit.threads=4;
        let mut g=Governor::new(limit);let mut first=b();first.host_bytes=u64::MAX;g.reserve("a",first).unwrap();
        assert_eq!(g.reserve("b",b()),Err(AdmissionError::Overflow));assert_eq!(g.used(),first);
    }
}
