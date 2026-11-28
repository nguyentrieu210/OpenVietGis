//! Conservative admission budgets from host-supplied, measured telemetry.
//! Discovery of OS memory/GPU telemetry and hard containment are separate gates.
use super::{Budget, Device};
pub const GIB: u64 = 1 << 30;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Profile { Lite, Balanced, Survey }
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Snapshot {
    pub physical_bytes: u64, pub available_bytes: u64,
    pub volume_bytes: u64, pub free_bytes: u64, pub logical_cpus: u16,
    pub gpu: Option<Gpu>,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Gpu {
    pub device: Device, pub total_bytes: u64, pub available_bytes: u64,
    pub unified_memory: bool, pub qualified: bool,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Admission {
    pub profile: Profile, pub device: Device, pub budget: Budget,
    pub shared_pool_bytes: Option<u64>, pub output_reserved_bytes: u64,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Error { InvalidTelemetry, InsufficientMemory, InsufficientDisk, UnqualifiedGpu }

/// Caller must atomically reserve all budgets across jobs; a snapshot is not a lease.
/// UMA is deliberately partitioned conservatively rather than counted twice.
pub fn recommend(s: Snapshot, p: Profile, requested: Device, output: u64) -> Result<Admission, Error> {
    if s.physical_bytes == 0 || s.available_bytes > s.physical_bytes ||
        s.volume_bytes == 0 || s.free_bytes > s.volume_bytes || s.logical_cpus == 0 {
        return Err(Error::InvalidTelemetry);
    }
    if let Some(g)=s.gpu {
        if g.device==Device::Cpu || g.total_bytes==0 || g.available_bytes>g.total_bytes {
            return Err(Error::InvalidTelemetry);
        }
    }
    let (minimum,cap,reserve,threads) = match p {
        Profile::Lite => (8*GIB,4*GIB,5*GIB/2,4),
        Profile::Balanced => (16*GIB,10*GIB,4*GIB,8),
        Profile::Survey => (32*GIB,24*GIB,6*GIB,16),
    };
    if s.physical_bytes < minimum { return Err(Error::InsufficientMemory); }
    let pool=s.physical_bytes.saturating_sub(reserve).min(s.available_bytes.saturating_sub(GIB)).min(cap);
    if pool < 2*GIB { return Err(Error::InsufficientMemory); }
    let disk_floor=(5*GIB).max(s.volume_bytes/10);
    let scratch=s.free_bytes.checked_sub(disk_floor).and_then(|b|b.checked_sub(output))
        .filter(|b|*b>=GIB).ok_or(Error::InsufficientDisk)?;
    let (host,device,shared)=if requested==Device::Cpu { (pool,0,None) } else {
        let g=s.gpu.filter(|g|g.device==requested && g.qualified).ok_or(Error::UnqualifiedGpu)?;
        let free=g.available_bytes.saturating_sub(GIB/2).min(g.total_bytes/4*3);
        let device=if g.unified_memory {free.min(pool/2)} else {free};
        let host=if g.unified_memory {pool-device} else {pool};
        if device<GIB || host<GIB {return Err(Error::InsufficientMemory);}
        (host,device,if g.unified_memory {Some(pool)} else {None})
    };
    Ok(Admission{profile:p,device:requested,budget:Budget{host_bytes:host,device_bytes:device,
        scratch_bytes:scratch,threads:s.logical_cpus.saturating_sub(1).max(1).min(threads)},
        shared_pool_bytes:shared,output_reserved_bytes:output})
}

#[cfg(test)]
mod tests {
    use super::*;
    fn snapshot()->Snapshot {Snapshot{physical_bytes:16*GIB,available_bytes:12*GIB,
        volume_bytes:100*GIB,free_bytes:30*GIB,logical_cpus:12,gpu:None}}
    #[test] fn reserves_system_and_disk() {
        let a=recommend(snapshot(),Profile::Balanced,Device::Cpu,2*GIB).unwrap();
        assert_eq!(a.budget.host_bytes,10*GIB);assert_eq!(a.budget.scratch_bytes,18*GIB);
        assert_eq!(a.budget.threads,8);assert_eq!(a.budget.device_bytes,0);
    }
    #[test] fn unqualified_gpu_never_selected() {
        assert_eq!(recommend(snapshot(),Profile::Balanced,Device::Cuda,0),Err(Error::UnqualifiedGpu));
    }
    #[test] fn unified_memory_not_double_counted() {
        let mut s=snapshot();s.gpu=Some(Gpu{device:Device::Metal,total_bytes:16*GIB,available_bytes:10*GIB,
            unified_memory:true,qualified:true});
        let a=recommend(s,Profile::Balanced,Device::Metal,0).unwrap();
        assert_eq!(a.budget.host_bytes+a.budget.device_bytes,a.shared_pool_bytes.unwrap());
    }
    #[test] fn refuses_low_disk_and_invalid_telemetry() {
        let mut s=snapshot();s.free_bytes=10*GIB;
        assert_eq!(recommend(s,Profile::Lite,Device::Cpu,0),Err(Error::InsufficientDisk));
        s.available_bytes=17*GIB;
        assert_eq!(recommend(s,Profile::Lite,Device::Cpu,0),Err(Error::InvalidTelemetry));
    }
}
