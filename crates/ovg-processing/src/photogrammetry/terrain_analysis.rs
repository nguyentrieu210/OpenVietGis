//! Streaming co-registered metric raster analysis. Trusted host supplies CRS,
//! vertical datum and immutable raw float32 raster provenance. No reprojection.
use super::ortho::Grid;
use sha2::{Digest,Sha256};
use std::{fs::{self,File,OpenOptions},io::{Read,Write},path::Path};
#[derive(Debug)]pub enum Error{Invalid,Limit,Coverage,Cancelled,Corrupt,Io(std::io::Error)}
impl From<std::io::Error> for Error{fn from(e:std::io::Error)->Self{Self::Io(e)}}
pub struct Raster<'a>{pub path:&'a Path,pub sha256:&'a str,pub grid:Grid,pub epsg:u16,pub vertical_datum:&'a str}
#[derive(Debug,Clone,Copy)]pub struct Policy{pub max_cells:u64,pub min_coverage:f64,pub max_abs_z_m:f64,pub deadband_m:f64}
#[derive(Debug,Clone,Copy,PartialEq)]pub struct Stats{
 pub total_cells:u64,pub valid_cells:u64,pub missing_cells:u64,pub valid_area_m2:f64,pub missing_area_m2:f64,
 pub mean_delta_m:f64,pub min_delta_m:f64,pub max_delta_m:f64,
 pub cut_m3:f64,pub fill_m3:f64,pub net_m3:f64,pub unchanged_area_m2:f64,
}
#[derive(Debug)]pub struct Chm{pub sha256:String,pub stats:Stats,pub negative_clamped:u64}
#[derive(Default)]struct Sum{sum:f64,c:f64}
impl Sum{fn add(&mut self,v:f64){let y=v-self.c;let t=self.sum+y;self.c=(t-self.sum)-y;self.sum=t;}}
#[derive(Clone,Copy)]enum Mode{Volume,Canopy}
fn validate(a:&Raster<'_>,b:&Raster<'_>,p:Policy)->Result<(u64,f64),Error>{
 let g=a.grid;let h=b.grid;
 if !a.path.is_absolute()||!b.path.is_absolute()||!super::digest(a.sha256)||!super::digest(b.sha256)||
 !((32601..=32660).contains(&a.epsg)||(32701..=32760).contains(&a.epsg))||a.epsg!=b.epsg||
 a.vertical_datum.is_empty()||a.vertical_datum.len()>128||a.vertical_datum.chars().any(char::is_control)||a.vertical_datum!=b.vertical_datum||
 g.width!=h.width||g.height!=h.height||g.origin_x!=h.origin_x||g.origin_y!=h.origin_y||g.gsd!=h.gsd||
 !g.origin_x.is_finite()||!g.origin_y.is_finite()||!g.gsd.is_finite()||g.gsd<=0.||g.width==0||g.height==0||
 !p.min_coverage.is_finite()||p.min_coverage<=0.||p.min_coverage>1.||!p.deadband_m.is_finite()||p.deadband_m<0.||
 !p.max_abs_z_m.is_finite()||p.max_abs_z_m<=0.||p.max_abs_z_m>100000.||p.deadband_m>p.max_abs_z_m{return Err(Error::Invalid);}
 let n=u64::from(g.width)*u64::from(g.height);let area=g.gsd*g.gsd;
 if n>p.max_cells||n>1_000_000_000||!area.is_finite()||area<=0.||!(area*n as f64*p.max_abs_z_m*2.).is_finite(){return Err(Error::Limit);}
 for r in [a,b]{let m=fs::symlink_metadata(r.path)?;if !m.file_type().is_file()||m.len()!=n*4{return Err(Error::Corrupt);}}
 Ok((n,area))
}
fn compute(a:&Raster<'_>,b:&Raster<'_>,p:Policy,mode:Mode,mut out:Option<&mut File>,mut stop:impl FnMut()->bool)->Result<(Stats,String,u64),Error>{
 let(n,area)=validate(a,b,p)?;if stop(){return Err(Error::Cancelled);}
 let(mut f,mut g)=(File::open(a.path)?,File::open(b.path)?);
 let(mut ha,mut hb,mut hc)=(Sha256::new(),Sha256::new(),Sha256::new());
 let(mut aa,mut bb,mut cc)=(vec![0u8;65536],vec![0u8;65536],Vec::with_capacity(65536));
 let(mut done,mut valid,mut unchanged,mut clamped)=(0u64,0u64,0u64,0u64);
 let(mut cut,mut fill,mut delta)=(Sum::default(),Sum::default(),Sum::default());
 let(mut min,mut max)=(f64::INFINITY,f64::NEG_INFINITY);
 while done<n{
 if stop(){return Err(Error::Cancelled);}
 let count=(n-done).min(16384)as usize;let len=count*4;
 f.read_exact(&mut aa[..len])?;g.read_exact(&mut bb[..len])?;
 ha.update(&aa[..len]);hb.update(&bb[..len]);cc.clear();
 for i in 0..count{
 let x=f32::from_le_bytes(aa[i*4..i*4+4].try_into().unwrap());let y=f32::from_le_bytes(bb[i*4..i*4+4].try_into().unwrap());
 if x.is_infinite()||y.is_infinite()||(x.is_finite()&&f64::from(x).abs()>p.max_abs_z_m)||(y.is_finite()&&f64::from(y).abs()>p.max_abs_z_m){return Err(Error::Invalid);}
 let mut v=f32::NAN;
 if x.is_finite()&&y.is_finite(){
 let mut d=f64::from(y)-f64::from(x);
 if matches!(mode,Mode::Canopy)&&d<0.{
 // Small negative heights within the stated tolerance are clamped.
 // Larger inversions are invalid for CHM, not silently set to zero.
 if d.abs()>p.deadband_m{if out.is_some(){cc.extend(f32::NAN.to_le_bytes());}continue;}
 d=0.;clamped+=1;
 }
 valid+=1;delta.add(d);min=min.min(d);max=max.max(d);
 if d.abs()<=p.deadband_m{unchanged+=1;}else if d>0.{fill.add(d*area);}else{cut.add(-d*area);}
 v=d as f32;
 }
 if out.is_some(){cc.extend(v.to_le_bytes());}
 }
 if let Some(w)=out.as_deref_mut(){w.write_all(&cc)?;hc.update(&cc);}
 done+=count as u64;
 }
 let mut tail=[0];if f.read(&mut tail)?!=0||g.read(&mut tail)?!=0{return Err(Error::Corrupt);}
 if format!("{:x}",ha.finalize())!=a.sha256||format!("{:x}",hb.finalize())!=b.sha256{return Err(Error::Corrupt);}
 if stop(){return Err(Error::Cancelled);}
 if valid==0||(valid as f64)/(n as f64)<p.min_coverage{return Err(Error::Coverage);}
 let stats=Stats{total_cells:n,valid_cells:valid,missing_cells:n-valid,valid_area_m2:valid as f64*area,missing_area_m2:(n-valid)as f64*area,
 mean_delta_m:delta.sum/valid as f64,min_delta_m:min,max_delta_m:max,cut_m3:cut.sum,fill_m3:fill.sum,net_m3:fill.sum-cut.sum,unchanged_area_m2:unchanged as f64*area};
 Ok((stats,format!("{:x}",hc.finalize()),clamped))
}
/// Signed after-before; positive is fill, negative is cut. Deadband excludes
/// entire small changes, not just the threshold portion. No extrapolation.
pub fn cut_fill(before:&Raster<'_>,after:&Raster<'_>,p:Policy,stop:impl FnMut()->bool)->Result<Stats,Error>{
 Ok(compute(before,after,p,Mode::Volume,None,stop)?.0)
}
/// CHM = DSM - DTM. The same grid, units and datum are mandatory. Output
/// is raw float32 metres, NaN for missing/inconsistent cells. Fresh path only.
pub fn canopy_height(dtm:&Raster<'_>,dsm:&Raster<'_>,output:&Path,p:Policy,mut stop:impl FnMut()->bool)->Result<Chm,Error>{
 if !output.is_absolute()||output.exists(){return Err(Error::Invalid);}
 let parent=output.parent().ok_or(Error::Invalid)?;let temp=parent.join(format!(".chm-{}",uuid::Uuid::new_v4()));
 let mut f=OpenOptions::new().write(true).create_new(true).open(&temp)?;
 let q=(||{
 let(stats,sha256,negative_clamped)=compute(dtm,dsm,p,Mode::Canopy,Some(&mut f),&mut stop)?;
 f.sync_all()?;drop(f);if stop(){return Err(Error::Cancelled);}fs::hard_link(&temp,output)?;
 Ok(Chm{sha256,stats,negative_clamped})
 })();
 let _=fs::remove_file(temp);q
}
#[cfg(test)]mod tests{
 use super::*;
 fn grid()->Grid{Grid{origin_x:500000.,origin_y:1200000.,gsd:2.,width:2,height:2}}
 fn p()->Policy{Policy{max_cells:4,min_coverage:0.5,max_abs_z_m:1000.,deadband_m:0.1}}
 fn fixture()->(std::path::PathBuf,String,String){
 let d=std::env::temp_dir().join(format!("ovg-analysis-{}",uuid::Uuid::new_v4()));fs::create_dir(&d).unwrap();
 let write=|n:&str,a:[f32;4]|{let b:Vec<u8>=a.iter().flat_map(|v|v.to_le_bytes()).collect();fs::write(d.join(n),&b).unwrap();format!("{:x}",Sha256::digest(&b))};
 let a=write("a",[10.,10.,10.,f32::NAN]);let b=write("b",[12.,9.,10.,20.]);(d,a,b)
 }
 #[test]fn volume_sign_and_missing_area(){let(d,a,b)=fixture();
 let ap=d.join("a");let bp=d.join("b");let x=Raster{path:&ap,sha256:&a,grid:grid(),epsg:32648,vertical_datum:"test"};
 let y=Raster{path:&bp,sha256:&b,..x};let q=cut_fill(&x,&y,p(),||false).unwrap();
 assert_eq!((q.cut_m3,q.fill_m3,q.net_m3),(4.,8.,4.));assert_eq!(q.missing_area_m2,4.);assert_eq!(q.valid_area_m2,12.);
 let _=fs::remove_dir_all(d);}
 #[test]fn chm_rejects_large_inversion_and_checks_digest(){let(d,a,b)=fixture();
 let ap=d.join("a");let bp=d.join("b");let x=Raster{path:&ap,sha256:&a,grid:grid(),epsg:32648,vertical_datum:"test"};
 let y=Raster{path:&bp,sha256:&b,..x};let out=d.join("chm");
 let q=canopy_height(&x,&y,&out,p(),||false).unwrap();assert_eq!(q.stats.valid_cells,2);
 let bytes=fs::read(&out).unwrap();let v:Vec<_>=bytes.chunks_exact(4).map(|b|f32::from_le_bytes(b.try_into().unwrap())).collect();
 assert_eq!(v[0],2.);assert!(v[1].is_nan()&&v[3].is_nan());assert_eq!(v[2],0.);
 let y=Raster{sha256:&a,..y};assert!(matches!(canopy_height(&x,&y,&d.join("bad"),p(),||false),Err(Error::Corrupt)));assert!(!d.join("bad").exists());
 let _=fs::remove_dir_all(d);}
}
