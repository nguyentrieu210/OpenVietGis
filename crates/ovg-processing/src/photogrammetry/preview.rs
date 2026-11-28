//! Bounded overview of our own canonical GeoTIFF shards. No arbitrary TIFF decoder.
//! Four UTM corners are a display approximation, not a resampled Web Mercator tile.
use super::{disk_store,geotiff,ortho};
use serde::Serialize;
use std::path::Path;
#[derive(Debug)]pub enum Error{Invalid,Limit,Cancelled,Store(disk_store::Error)}
#[derive(Debug,Clone,Serialize)]pub struct Info{pub width:u32,pub height:u32,pub corners:[[f64;2];4]}
pub struct Overview{pub info:Info,pub rgba:Vec<u8>}
/// WGS84 inverse UTM series, restricted to normal zone extents.
pub fn lonlat(epsg:u16,e:f64,n:f64)->Result<[f64;2],Error>{
 let(zone,south)=match epsg{32601..=32660=>(epsg-32600,false),32701..=32760=>(epsg-32700,true),_=>return Err(Error::Invalid)};
 if !e.is_finite()||!n.is_finite()||!(100000. ..=900000.).contains(&e)||!(0. ..=10000000.).contains(&n){return Err(Error::Invalid);}
 let a=6378137.0f64;let f=1./298.257223563;let es=f*(2.-f);let ep=es/(1.-es);let k=0.9996;
 let y=if south{n-10000000.}else{n};let m=y/k;
 let mu=m/(a*(1.-es/4.-3.*es.powi(2)/64.-5.*es.powi(3)/256.));
 let q=(1.-(1.-es).sqrt())/(1.+(1.-es).sqrt());
 let phi=mu+(3.*q/2.-27.*q.powi(3)/32.)*(2.*mu).sin()+
 (21.*q.powi(2)/16.-55.*q.powi(4)/32.)*(4.*mu).sin()+
 (151.*q.powi(3)/96.)*(6.*mu).sin()+(1097.*q.powi(4)/512.)*(8.*mu).sin();
 let s=phi.sin();let c=phi.cos();let t=phi.tan().powi(2);let cc=ep*c*c;
 let nu=a/(1.-es*s*s).sqrt();let rho=a*(1.-es)/(1.-es*s*s).powf(1.5);let d=(e-500000.)/(nu*k);
 let lat=phi-(nu*phi.tan()/rho)*(d*d/2.-(5.+3.*t+10.*cc-4.*cc*cc-9.*ep)*d.powi(4)/24.+
 (61.+90.*t+298.*cc+45.*t*t-252.*ep-3.*cc*cc)*d.powi(6)/720.);
 let lon=(f64::from(zone)*6.-183.)+(d-(1.+2.*t+cc)*d.powi(3)/6.+
 (5.-2.*cc+28.*t-3.*cc*cc+8.*ep+24.*t*t)*d.powi(5)/120.).to_degrees()/c;
 let lat=lat.to_degrees();
 if !lon.is_finite()||!lat.is_finite()||!(-180. ..=180.).contains(&lon)||!(-80. ..=84.).contains(&lat)||
 (south&&lat>1e-7)||(!south&&lat< -1e-7){return Err(Error::Invalid);}
 Ok([lon,lat])
}
fn f64_at(b:&[u8],i:usize)->f64{f64::from_le_bytes(b[i..i+8].try_into().unwrap())}
/// Nearest-neighbour overview sampled at pixel centres, sequential tile I/O.
/// Validates canonical TIFF layout, hash, shared affine and exact sample coverage.
pub fn read(root:&Path,max_side:u32,mut cancelled:impl FnMut()->bool)->Result<Overview,Error>{
 if max_side==0||max_side>1024{return Err(Error::Limit);}
 let store=disk_store::Store::open(root).map_err(Error::Store)?;
 if store.head().map_err(Error::Store)?!=1{return Err(Error::Invalid);}
 let rev=store.revision(1).map_err(Error::Store)?;
 if rev.grid_id!="preview"||rev.tiles.len()>8192{return Err(Error::Invalid);}
 let(mut w,mut h,mut area,mut bytes)=(0u32,0u32,0u64,0u64);
 for t in rev.tiles.values(){
 let q=t.window;if q.width==0||q.height==0||q.width>512||q.height>512{return Err(Error::Invalid);}
 w=w.max(q.x.checked_add(q.width).ok_or(Error::Limit)?);h=h.max(q.y.checked_add(q.height).ok_or(Error::Limit)?);
 let n=u64::from(q.width)*u64::from(q.height);
 if t.bytes!=304+4*n{return Err(Error::Invalid);}area+=n;bytes+=t.bytes;
 }
 if area!=u64::from(w)*u64::from(h)||area>64_000_000||bytes>300*1024*1024{return Err(Error::Limit);}
 let step=w.max(h).div_ceil(max_side).max(1);let ow=w.div_ceil(step);let oh=h.div_ceil(step);
 let mut rgba=vec![0u8;(ow*oh*4)as usize];let mut seen=vec![false;(ow*oh)as usize];
 let mut affine:Option<(f64,f64,f64)>=None;
 let sample=|i:u32,limit:u32| (u64::from(i)*u64::from(step)+u64::from(step/2)).min(u64::from(limit-1)) as u32;
 for t in rev.tiles.values(){
 if cancelled(){return Err(Error::Cancelled);}
 let q=t.window;
 let path=store.verified_path(&t.sha256,t.bytes).map_err(Error::Store)?;
 let b=disk_store::bounded_read(&path,t.bytes).map_err(Error::Store)?;
 if b.len() as u64!=t.bytes||disk_store::hash(&b)!=t.sha256{return Err(Error::Invalid);}
 let gsd=f64_at(&b,192);let x=f64_at(&b,240);let y=f64_at(&b,248);
 let grid=ortho::Grid{origin_x:x,origin_y:y,gsd,width:q.width,height:q.height};
 let canonical=geotiff::rgba(grid,rev.epsg,&b[304..],t.bytes).map_err(|_|Error::Invalid)?;
 if canonical!=b{return Err(Error::Invalid);}
 let a=(x-f64::from(q.x)*gsd,y+f64::from(q.y)*gsd,gsd);
 if let Some(old)=affine{
 if old.2!=gsd||(old.0-a.0).abs()>gsd*1e-5||(old.1-a.1).abs()>gsd*1e-5{return Err(Error::Invalid);}
 }else{affine=Some(a);}
 // Include at most one neighbouring overview sample at each tile boundary.
 let ix0=(q.x/step).saturating_sub(1);let ix1=((q.x+q.width).div_ceil(step)+1).min(ow);
 let iy0=(q.y/step).saturating_sub(1);let iy1=((q.y+q.height).div_ceil(step)+1).min(oh);
 for iy in iy0..iy1{let sy=sample(iy,h);if sy<q.y||sy>=q.y+q.height{continue;}
 for ix in ix0..ix1{let sx=sample(ix,w);if sx<q.x||sx>=q.x+q.width{continue;}
 let i=(iy*ow+ix)as usize;if seen[i]{return Err(Error::Invalid);}seen[i]=true;
 let j=304+(((sy-q.y)*q.width+(sx-q.x))*4)as usize;
 rgba[4*i..4*i+4].copy_from_slice(&b[j..j+4]);
 }}
 }
 if seen.iter().any(|v|!*v){return Err(Error::Invalid);}
 let(x,y,s)=affine.ok_or(Error::Invalid)?;let right=x+f64::from(w)*s;let bottom=y-f64::from(h)*s;
 // Four-corner map approximation only for bounded local extents.
 if f64::from(w.max(h))*s>20000.{return Err(Error::Limit);}
 let corners=[lonlat(rev.epsg,x,y)?,lonlat(rev.epsg,right,y)?,lonlat(rev.epsg,right,bottom)?,lonlat(rev.epsg,x,bottom)?];
 if cancelled(){return Err(Error::Cancelled);}
 Ok(Overview{info:Info{width:ow,height:oh,corners},rgba})
}
#[cfg(test)]mod tests{
 use super::*;
 #[test]fn central_meridian_and_southern_false_northing(){
 let q=lonlat(32648,500000.,0.).unwrap();assert!((q[0]-105.).abs()<1e-8&&q[1].abs()<1e-8);
 let q=lonlat(32748,500000.,10000000.).unwrap();assert!(q[1].abs()<1e-8);
 assert!(lonlat(4326,500000.,0.).is_err());}
 #[test]fn overview_reads_verified_rgba(){
 let p=std::env::temp_dir().join(format!("ovg-overview-{}",uuid::Uuid::new_v4()));
 let s=disk_store::Store::open(&p).unwrap();let g=ortho::Grid{origin_x:500000.,origin_y:1200000.,gsd:1.,width:2,height:2};
 let pixels=[10u8,20,30,255].repeat(4);let b=geotiff::rgba(g,32648,&pixels,1024).unwrap();let(hash,bytes)=s.put(&b).unwrap();
 s.publish(0,"preview",32648,[("t".into(),disk_store::TileRef{window:super::super::tiles::Window{x:0,y:0,width:2,height:2},sha256:hash,bytes})].into_iter().collect()).unwrap();
 let o=read(&p,1,||false).unwrap();assert_eq!(o.rgba,vec![10,20,30,255]);assert_eq!(o.info.width,1);
 let _=std::fs::remove_dir_all(p);}
}
