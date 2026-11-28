//! Stream a verified fused PLY into a bounded DSM in a known metric frame.
//! No interpolation or DTM classification. Empty/under-supported cells stay NaN.
use super::{fused_ply,colmap_model::Alignment,ortho::Grid};
use std::{io::{Read,Write},fs::{self,OpenOptions},path::Path};
use sha2::{Digest,Sha256};
#[derive(Debug)]pub enum Error{Invalid,Limit,Cancelled,Coverage,Cloud(fused_ply::Error),Ground(super::ground_filter::Error),Pmf(super::pmf::Error),Io(std::io::Error)}
impl From<std::io::Error> for Error{fn from(e:std::io::Error)->Self{Self::Io(e)}}
#[derive(Debug,Clone,Copy)]pub struct Policy{pub cloud:fused_ply::Policy,pub min_points:u32,pub min_coverage:f64,pub max_work_bytes:u64,pub min_z:f64,pub max_z:f64}
#[derive(Debug,Clone)]pub struct Receipt{pub sha256:String,pub points:u64,pub inside:u64,pub cells:u64,pub covered:u64,pub min_z:f32,pub max_z:f32}
pub(super) fn valid_alignment(a:Alignment)->bool{
 if !a.scale.is_finite()||a.scale<=0.||!a.offset.iter().chain(a.rotation.iter().flatten()).all(|v|v.is_finite()){return false;}
 for i in 0..3{for j in 0..3{
 let d=(0..3).map(|k|a.rotation[i][k]*a.rotation[j][k]).sum::<f64>();
 if (d-if i==j{1.}else{0.}).abs()>1e-6{return false;}
 }}
 let r=a.rotation;let det=r[0][0]*(r[1][1]*r[2][2]-r[1][2]*r[2][1])-r[0][1]*(r[1][0]*r[2][2]-r[1][2]*r[2][0])+r[0][2]*(r[1][0]*r[2][1]-r[1][1]*r[2][0]);
 (det-1.).abs()<=1e-6
}
/// Fresh output path, atomic no-overwrite publication. Input must be a host
/// authorized immutable PLY in the alignment's local reconstruction frame.
pub fn build(reader:impl Read,output:&Path,g:Grid,a:Alignment,p:Policy,cancelled:impl FnMut()->bool)->Result<Receipt,Error>{
 build_inner(reader,output,g,a,p,None,cancelled)
}
/// DTM candidate from lower cell envelope with conservative terrain support.
/// Not individual point classification or survey-grade bare-earth certification.
pub fn build_dtm(reader:impl Read,output:&Path,g:Grid,a:Alignment,p:Policy,
 ground:super::ground_filter::Policy,cancelled:impl FnMut()->bool)->Result<Receipt,Error>{
 build_inner(reader,output,g,a,p,Some(GroundMethod::Directional(ground)),cancelled)
}
pub fn build_pmf_dtm(reader:impl Read,output:&Path,g:Grid,a:Alignment,p:Policy,
 ground:super::pmf::Policy,cancelled:impl FnMut()->bool)->Result<Receipt,Error>{
 build_inner(reader,output,g,a,p,Some(GroundMethod::Morphological(ground)),cancelled)
}
enum GroundMethod{Directional(super::ground_filter::Policy),Morphological(super::pmf::Policy)}
fn build_inner(reader:impl Read,output:&Path,g:Grid,a:Alignment,p:Policy,ground:Option<GroundMethod>,mut cancelled:impl FnMut()->bool)->Result<Receipt,Error>{
 if !output.is_absolute()||g.width==0||g.height==0||!g.gsd.is_finite()||g.gsd<=0.||
 !g.origin_x.is_finite()||!g.origin_y.is_finite()||!valid_alignment(a)||p.min_points==0||
 !p.min_coverage.is_finite()||p.min_coverage<=0.||p.min_coverage>1.||
 !p.min_z.is_finite()||!p.max_z.is_finite()||p.min_z>=p.max_z||p.min_z< -(f32::MAX as f64)||p.max_z>f32::MAX as f64{return Err(Error::Invalid);}
 let n=u64::from(g.width)*u64::from(g.height);
 if n>16_000_000||n.checked_mul(if matches!(&ground,Some(GroundMethod::Morphological(_))){32}else if ground.is_some(){16}else{8}).and_then(|v|v.checked_add(256*1024)).ok_or(Error::Limit)?>p.max_work_bytes{return Err(Error::Limit);}
 if cancelled(){return Err(Error::Cancelled);}
 let mut z=vec![f32::NAN;n as usize];let mut counts=vec![0u32;n as usize];let mut inside=0u64;
 let summary=fused_ply::visit(reader,p.cloud,&mut cancelled,|xyz|{
 let v:[f64;3]=std::array::from_fn(|i|a.offset[i]+a.scale*(0..3).map(|j|a.rotation[i][j]*xyz[j]).sum::<f64>());
 if !v.iter().all(|x|x.is_finite()){return Err(fused_ply::Error::Invalid);}
 let x=(v[0]-g.origin_x)/g.gsd;let y=(g.origin_y-v[1])/g.gsd;
 if x<0.||y<0.||x>=f64::from(g.width)||y>=f64::from(g.height){return Ok(());}
 if v[2]<p.min_z||v[2]>p.max_z{return Err(fused_ply::Error::Invalid);}
 let i=y.floor()as usize*g.width as usize+x.floor()as usize;
 inside+=1;counts[i]=counts[i].checked_add(1).ok_or(fused_ply::Error::Limit)?;
 let h=v[2]as f32;if z[i].is_nan()||if ground.is_some(){h<z[i]}else{h>z[i]}{z[i]=h;}Ok(())
 }).map_err(|e|if matches!(e,fused_ply::Error::Cancelled){Error::Cancelled}else{Error::Cloud(e)})?;
 for(i,(v,c))in z.iter_mut().zip(counts).enumerate(){
 if i%65536==0&&cancelled(){return Err(Error::Cancelled);}
 if c<p.min_points{*v=f32::NAN;}
 }
 match ground{
 Some(GroundMethod::Directional(q))=>{z=super::ground_filter::filter(g,&z,q,&mut cancelled).map_err(|e|
 if matches!(e,super::ground_filter::Error::Cancelled){Error::Cancelled}else{Error::Ground(e)})?.elevations;},
 Some(GroundMethod::Morphological(q))=>{z=super::pmf::filter(g,&z,q,&mut cancelled).map_err(|e|
 if matches!(e,super::pmf::Error::Cancelled){Error::Cancelled}else{Error::Pmf(e)})?.elevations;},
 None=>{}
 }
 let(mut covered,mut low,mut high)=(0u64,f32::INFINITY,f32::NEG_INFINITY);
 for v in &z{if v.is_finite(){covered+=1;low=low.min(*v);high=high.max(*v);}}
 if covered==0||(covered as f64)/(n as f64)<p.min_coverage{return Err(Error::Coverage);}
 let parent=output.parent().ok_or(Error::Invalid)?;
 let temp=parent.join(format!(".dsm-{}",uuid::Uuid::new_v4()));
 let mut f=OpenOptions::new().write(true).create_new(true).open(&temp)?;
 let result=(||{
 let mut h=Sha256::new();let mut buf=Vec::with_capacity(64*1024);
 for rows in z.chunks(16384){if cancelled(){return Err(Error::Cancelled);}buf.clear();
 for v in rows{buf.extend(v.to_le_bytes());}h.update(&buf);f.write_all(&buf)?;}
 f.sync_all()?;drop(f);
 if cancelled(){return Err(Error::Cancelled);}
 fs::hard_link(&temp,output)?;
 Ok(Receipt{sha256:format!("{:x}",h.finalize()),points:summary.count,inside,cells:n,covered,min_z:low,max_z:high})
 })();
 let _=fs::remove_file(temp);result
}
#[cfg(test)]mod tests{
 use super::*;
 fn ply()->Vec<u8>{let mut b=b"ply\nformat binary_little_endian 1.0\nelement vertex 3\nproperty float x\nproperty float y\nproperty float z\nproperty uchar red\nproperty uchar green\nproperty uchar blue\nend_header\n".to_vec();
 for p in [[0.5f32,1.5,3.],[0.5,1.5,5.],[1.5,0.5,7.]]{for v in p{b.extend(v.to_le_bytes());}b.extend([0,0,0]);}b}
 fn grid()->Grid{Grid{origin_x:500000.,origin_y:1200002.,gsd:1.,width:2,height:2}}
 fn align()->Alignment{Alignment{scale:1.,rotation:[[1.,0.,0.],[0.,1.,0.],[0.,0.,1.]],offset:[500000.,1200000.,100.]}}
 fn p()->Policy{Policy{cloud:fused_ply::Policy{max_points:10,max_bytes:4096,max_abs_coordinate:100.},min_points:1,min_coverage:0.5,max_work_bytes:1<<20,min_z:0.,max_z:1000.}}
 #[test]fn dsm_max_and_nodata(){let d=std::env::temp_dir().join(format!("ovg-dsm-{}",uuid::Uuid::new_v4()));fs::create_dir(&d).unwrap();let out=d.join("dem.raw");
 let b=ply();let r=build(b.as_slice(),&out,grid(),align(),p(),||false).unwrap();
 assert_eq!(r.covered,2);assert_eq!(r.inside,3);let b=fs::read(out).unwrap();let v:Vec<_>=b.chunks_exact(4).map(|q|f32::from_le_bytes(q.try_into().unwrap())).collect();
 assert_eq!(v[0],105.);assert!(v[1].is_nan()&&v[2].is_nan());assert_eq!(v[3],107.);let _=fs::remove_dir_all(d);}
 #[test]fn inadequate_coverage_does_not_publish(){let d=std::env::temp_dir().join(format!("ovg-dsm-{}",uuid::Uuid::new_v4()));fs::create_dir(&d).unwrap();let out=d.join("dem.raw");
 assert!(matches!(build(ply().as_slice(),&out,grid(),align(),Policy{min_coverage:0.9,..p()},||false),Err(Error::Coverage)));assert!(!out.exists());let _=fs::remove_dir_all(d);}
}
