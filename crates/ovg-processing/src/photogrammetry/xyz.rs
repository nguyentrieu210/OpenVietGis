//! On-demand Web Mercator raster tiles from immutable WGS84 UTM shards.
//! Nearest-neighbour inverse mapping at requested XYZ zoom, no full raster buffer.
use super::{disk_store,geotiff,ortho,preview,tiles::Window};
use std::{collections::BTreeMap,path::Path};
pub const SIDE:u32=256;
#[derive(Debug)]pub enum Error{Invalid,Limit,Cancelled,Store(disk_store::Error)}
fn forward(epsg:u16,lon:f64,lat:f64)->Option<[f64;2]>{
 let(zone,south)=match epsg{32601..=32660=>(epsg-32600,false),32701..=32760=>(epsg-32700,true),_=>return None};
 let center=f64::from(zone)*6.-183.;
 if !lon.is_finite()||!lat.is_finite()||!( -80. ..=84.).contains(&lat)||
 (south&&lat>0.)||(!south&&lat<0.)||(lon-center).abs()>6.{return None;}
 let a=6378137.0f64;let f=1./298.257223563;let es=f*(2.-f);let ep=es/(1.-es);let k=0.9996;
 let phi=lat.to_radians();let(s,c)=phi.sin_cos();let t=phi.tan().powi(2);let cc=ep*c*c;
 let nu=a/(1.-es*s*s).sqrt();let d=(lon-center).to_radians()*c;
 let m=a*((1.-es/4.-3.*es.powi(2)/64.-5.*es.powi(3)/256.)*phi
 -(3.*es/8.+3.*es.powi(2)/32.+45.*es.powi(3)/1024.)*(2.*phi).sin()
 +(15.*es.powi(2)/256.+45.*es.powi(3)/1024.)*(4.*phi).sin()
 -(35.*es.powi(3)/3072.)*(6.*phi).sin());
 let e=500000.+k*nu*(d+(1.-t+cc)*d.powi(3)/6.+(5.-18.*t+t*t+72.*cc-58.*ep)*d.powi(5)/120.);
 let n=k*(m+nu*phi.tan()*(d*d/2.+(5.-t+9.*cc+4.*cc*cc)*d.powi(4)/24.+
 (61.-58.*t+t*t+600.*cc-330.*ep)*d.powi(6)/720.))+if south{10000000.}else{0.};
 if !e.is_finite()||!n.is_finite(){return None;}Some([e,n])
}
fn read_shard(store:&disk_store::Store,t:&disk_store::TileRef,epsg:u16)->Result<(ortho::Grid,Vec<u8>),Error>{
 let q=t.window;let expected=304+4*u64::from(q.width)*u64::from(q.height);
 if q.width==0||q.height==0||q.width>512||q.height>512||t.bytes!=expected{return Err(Error::Invalid);}
 let path=store.verified_path(&t.sha256,t.bytes).map_err(Error::Store)?;
 let b=disk_store::bounded_read(&path,t.bytes).map_err(Error::Store)?;
 if b.len() as u64!=t.bytes||disk_store::hash(&b)!=t.sha256{return Err(Error::Invalid);}
 let at=|i:usize|f64::from_le_bytes(b[i..i+8].try_into().unwrap());
 let g=ortho::Grid{origin_x:at(240),origin_y:at(248),gsd:at(192),width:q.width,height:q.height};
 if geotiff::rgba(g,epsg,&b[304..],t.bytes).map_err(|_|Error::Invalid)?!=b{return Err(Error::Invalid);}
 Ok((g,b))
}
fn contains(q:Window,x:u32,y:u32)->bool{x>=q.x&&y>=q.y&&x-q.x<q.width&&y-q.y<q.height}
/// A 256x256 RGBA tile, transparent outside the footprint. Reads only shards
/// selected by output samples, each at most once per call. No persistent cache.
pub fn render(root:&Path,z:u8,x:u32,y:u32,mut cancelled:impl FnMut()->bool)->Result<Vec<u8>,Error>{
 if z>24||x>=1u32<<z||y>=1u32<<z{return Err(Error::Invalid);}
 if cancelled(){return Err(Error::Cancelled);}
 let store=disk_store::Store::open(root).map_err(Error::Store)?;
 if store.head().map_err(Error::Store)?!=1{return Err(Error::Invalid);}
 let rev=store.revision(1).map_err(Error::Store)?;
 if rev.grid_id!="preview"||rev.tiles.len()>8192{return Err(Error::Invalid);}
 let refs:Vec<_>=rev.tiles.values().collect();let first=refs.first().ok_or(Error::Invalid)?;
 let(g,_)=read_shard(&store,first,rev.epsg)?;
 let ox=g.origin_x-f64::from(first.window.x)*g.gsd;let oy=g.origin_y+f64::from(first.window.y)*g.gsd;
 let(mut w,mut h,mut area,mut total)=(0u32,0u32,0u64,0u64);
 let mut index=BTreeMap::<(u32,u32),Vec<usize>>::new();let mut entries=0usize;
 for(i,t)in refs.iter().enumerate(){
 if i%128==0&&cancelled(){return Err(Error::Cancelled);}
 let q=t.window;
 if q.width==0||q.height==0||q.width>512||q.height>512{return Err(Error::Invalid);}
 let ex=q.x.checked_add(q.width).ok_or(Error::Limit)?;let ey=q.y.checked_add(q.height).ok_or(Error::Limit)?;
 w=w.max(ex);h=h.max(ey);area+=u64::from(q.width)*u64::from(q.height);total+=t.bytes;
 if t.bytes!=304+4*u64::from(q.width)*u64::from(q.height)||total>300*1024*1024{return Err(Error::Limit);}
 for by in q.y/256..=(ey-1)/256{for bx in q.x/256..=(ex-1)/256{
 entries+=1;if entries>100_000{return Err(Error::Limit);}index.entry((bx,by)).or_default().push(i);
 }}
 }
 if area!=u64::from(w)*u64::from(h)||area>64_000_000||f64::from(w.max(h))*g.gsd>20000.{return Err(Error::Limit);}
 for (e,n) in [(ox,oy),(ox+f64::from(w)*g.gsd,oy-f64::from(h)*g.gsd)]{preview::lonlat(rev.epsg,e,n).map_err(|_|Error::Invalid)?;}
 let mut requests=BTreeMap::<usize,Vec<(usize,u32,u32)>>::new();let mut checks=0usize;
 let world=f64::from(1u32<<z)*f64::from(SIDE);
 for row in 0..SIDE{
 if cancelled(){return Err(Error::Cancelled);}
 let v=(f64::from(y)*f64::from(SIDE)+f64::from(row)+0.5)/world;
 let lat=(std::f64::consts::PI*(1.-2.*v)).sinh().atan().to_degrees();
 for col in 0..SIDE{
 let u=(f64::from(x)*f64::from(SIDE)+f64::from(col)+0.5)/world;
 let Some([e,n])=forward(rev.epsg,u*360.-180.,lat)else{continue;};
 let px=(e-ox)/g.gsd;let py=(oy-n)/g.gsd;
 if px<0.||py<0.||px>=f64::from(w)||py>=f64::from(h){continue;}
 let(sx,sy)=(px.floor()as u32,py.floor()as u32);
 let list=index.get(&(sx/256,sy/256)).ok_or(Error::Invalid)?;
 let mut found=None;
 for &i in list{checks+=1;if checks>8_000_000{return Err(Error::Limit);}
 if contains(refs[i].window,sx,sy){if found.replace(i).is_some(){return Err(Error::Invalid);}}}
 let i=found.ok_or(Error::Invalid)?;
 requests.entry(i).or_default().push(((row*SIDE+col)as usize,sx,sy));
 }}
 let mut out=vec![0u8;(SIDE*SIDE*4)as usize];
 for(i,samples)in requests{
 if cancelled(){return Err(Error::Cancelled);}
 let t=refs[i];let q=t.window;let(a,b)=read_shard(&store,t,rev.epsg)?;
 if a.gsd!=g.gsd||(a.origin_x-ox-f64::from(q.x)*g.gsd).abs()>g.gsd*1e-5||
 (a.origin_y-oy+f64::from(q.y)*g.gsd).abs()>g.gsd*1e-5{return Err(Error::Invalid);}
 for (i,sx,sy)in samples{let j=304+(((sy-q.y)*q.width+(sx-q.x))*4)as usize;out[4*i..4*i+4].copy_from_slice(&b[j..j+4]);}
 }
 if cancelled(){return Err(Error::Cancelled);}Ok(out)
}
#[cfg(test)]mod tests{
 use super::*;
 #[test]fn forward_inverse_reference(){let p=forward(32648,106.7,10.8).unwrap();let q=preview::lonlat(32648,p[0],p[1]).unwrap();
 assert!((q[0]-106.7).abs()<1e-7&&(q[1]-10.8).abs()<1e-7);
 let p=forward(32748,105.,-10.).unwrap();assert!((p[0]-500000.).abs()<1e-8);assert!((preview::lonlat(32748,p[0],p[1]).unwrap()[1]+10.).abs()<1e-7);}
 #[test]fn renders_zoom_tile_and_transparent_outside(){
 let p=std::env::temp_dir().join(format!("ovg-xyz-{}",uuid::Uuid::new_v4()));
 let s=disk_store::Store::open(&p).unwrap();let [e,n]=forward(32648,105.,10.).unwrap();
 let g=ortho::Grid{origin_x:e-128.,origin_y:n+128.,gsd:1.,width:256,height:256};
 let b=geotiff::rgba(g,32648,&[20,30,40,255].repeat(65536),2<<20).unwrap();let(sha256,bytes)=s.put(&b).unwrap();
 s.publish(0,"preview",32648,[("t".into(),disk_store::TileRef{window:Window{x:0,y:0,width:256,height:256},sha256,bytes})].into_iter().collect()).unwrap();
 let z=19;let size=f64::from(1u32<<z);let x=((105.+180.)/360.*size).floor()as u32;
 let y=((1.-10f64.to_radians().tan().asinh()/std::f64::consts::PI)/2.*size).floor()as u32;
 let a=render(&p,z,x,y,||false).unwrap();assert!(a.chunks_exact(4).any(|q|q==[20,30,40,255]));
 assert!(render(&p,z,0,0,||false).unwrap().iter().all(|v|*v==0));
 assert!(matches!(render(&p,z,x,y,||true),Err(Error::Cancelled)));let _=std::fs::remove_dir_all(p);
 }
}
