//! Conservative CPU preview tuning from measured host telemetry and image inventory.
//! Not an empirical benchmark. GPU candidates remain disabled until qualified.
use super::{hardware::{self,Snapshot,Profile,GIB},Device,images::Dimensions,tiles,pipeline,colmap};
const MIB:u64=1<<20;
#[derive(Debug,Clone,Copy)]pub struct Workload{pub images:u32,pub largest:Dimensions,pub input_bytes:u64,pub output_pixels:u64,pub ordered_flight:bool}
#[derive(Debug,Clone)]pub struct Plan{pub admission:hardware::Admission,pub sparse:colmap::Config,pub render:pipeline::Policy,pub batch:pipeline::BatchPolicy,pub decode_bytes:u64,pub cache_bytes:u64}
#[derive(Debug,Clone,Copy,PartialEq,Eq)]pub enum Error{Invalid,Limit,Resources(hardware::Error)}
pub fn plan(s:Snapshot,w:Workload)->Result<Plan,Error>{
 if w.images<2||w.images>4096||w.largest.width==0||w.largest.height==0||w.input_bytes==0||
 w.output_pixels==0||w.output_pixels>64_000_000||w.input_bytes>8*GIB{return Err(Error::Invalid);}
 if w.images>250&&!w.ordered_flight{return Err(Error::Limit);}
 let p=if s.physical_bytes>=32*GIB{Profile::Survey}else if s.physical_bytes>=16*GIB{Profile::Balanced}else{Profile::Lite};
 let output=w.output_pixels.checked_mul(4).and_then(|n|n.checked_add(8192*304)).ok_or(Error::Limit)?;
 let a=hardware::recommend(s,p,Device::Cpu,output).map_err(Error::Resources)?;
 // Raw, prepared images and worker scratch may coexist. This is an estimate,
 // not a substitute for measuring the provider and enforcing disk quota.
 let scratch=w.input_bytes.checked_mul(4).and_then(|v|v.checked_add(GIB)).ok_or(Error::Limit)?;
 if scratch>a.budget.scratch_bytes{return Err(Error::Limit);}
 let host=a.budget.host_bytes;
 let source=(host/4).min(2*GIB);
 let decode=(source/2).min(512*MIB);
 let cache=(source/4).min(256*MIB);
 let max_side=w.largest.width.max(w.largest.height);
 // Conservative bound for a square resized image, decoder overhead + compressed cap.
 let side= [8192u32,6144,4096,3072,2048,1600,1024,512].into_iter().map(|v|v.min(max_side))
 .find(|&v|v>=128&&u64::from(v)*u64::from(v)*16+128*MIB<=decode).ok_or(Error::Limit)?;
 let tile_side=if host>=8*GIB{512}else{256};
 let tile_work=64*MIB;
 let threads=usize::from(a.budget.threads).min(16);
 let fixed=source+threads as u64*2*MIB+8192*1024;
 let charge=source+tile_work+tile_side as u64*4;
 let in_flight=host.saturating_sub(fixed).checked_div(charge).unwrap_or(0).min(threads as u64).min(4)as usize;
 if in_flight==0{return Err(Error::Limit);}
 let pairing=if w.images<=250{colmap::Pairing::ExhaustiveSmall}else{colmap::Pairing::Sequential{overlap:20}};
 Ok(Plan{admission:a,sparse:colmap::Config{image_count:w.images,max_image_side:side,threads:a.budget.threads,device:Device::Cpu,gpu_index:None,pairing},
 render:pipeline::Policy{tiles:tiles::Policy{min_side:32,max_side:tile_side,halo_px:0,bytes_per_pixel:64,max_working_bytes:tile_work,max_tiles:8192,max_depth:20},
 source_bytes:source,tile_bytes:8*MIB,max_samples:100_000_000,output_bytes:output},
 batch:pipeline::BatchPolicy{threads,max_in_flight:in_flight,host_bytes:host},decode_bytes:decode,cache_bytes:cache})
}
/// Actual Linux memory telemetry; disk and qualified GPU telemetry are deliberately
/// supplied by host. No shell execution or invented defaults.
#[cfg(target_os="linux")]
pub fn linux_memory()->std::io::Result<(u64,u64)>{
 use std::io::Read;
 let mut s=String::new();std::fs::File::open("/proc/meminfo")?.take(64*1024).read_to_string(&mut s)?;
 parse_meminfo(&s).ok_or_else(||std::io::Error::new(std::io::ErrorKind::InvalidData,"missing memory telemetry"))
}
fn parse_meminfo(s:&str)->Option<(u64,u64)>{
 let(mut total,mut free)=(None,None);
 for l in s.lines(){let Some((key,v))=l.split_once(':')else{continue;};
 let dst=match key{"MemTotal"=>&mut total,"MemAvailable"=>&mut free,_=>continue};
 if dst.is_some(){return None;}let mut t=v.split_whitespace();let n=t.next()?.parse::<u64>().ok()?;
 if t.next()!=Some("kB")||t.next().is_some(){return None;}*dst=Some(n.checked_mul(1024)?);}
 let(t,a)=(total?,free?);if t==0||a>t{return None;}Some((t,a))
}
#[cfg(test)]mod tests{
 use super::*;
 fn snap()->Snapshot{Snapshot{physical_bytes:16*GIB,available_bytes:12*GIB,volume_bytes:200*GIB,free_bytes:100*GIB,logical_cpus:12,gpu:None}}
 fn work()->Workload{Workload{images:100,largest:Dimensions{width:6000,height:4000},input_bytes:GIB,output_pixels:16_000_000,ordered_flight:false}}
 #[test]fn bounded_cpu_plan(){let p=plan(snap(),work()).unwrap();assert_eq!(p.sparse.device,Device::Cpu);assert!(p.batch.max_in_flight<=4);assert!(p.decode_bytes<=p.render.source_bytes);assert!(p.sparse.max_image_side<=6000);}
 #[test]fn unordered_large_and_insufficient_disk(){assert!(matches!(plan(snap(),Workload{images:251,..work()}),Err(Error::Limit)));
 let mut s=snap();s.free_bytes=GIB;assert!(plan(s,work()).is_err());}
 #[test]fn parses_measured_memory(){assert_eq!(parse_meminfo("MemTotal: 100 kB\nMemAvailable: 25 kB\n"),Some((102400,25600)));assert_eq!(parse_meminfo("MemTotal: 1 kB\n"),None);}
}
