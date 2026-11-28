//! Render-only Mapbox vector tile v2 encoding; never mutates canonical geometry.
use geo::{BooleanOps, Coord, Geometry, LineString, MapCoords, MultiLineString, Polygon, Rect};
use ovg_spatial::{parse_xy_wkb,ValidationLimits};
const EXTENT:i32=4096;
const BUFFER:i32=64;
const MAX_FEATURES:u32=512;
const MAX_TILE:usize=4*1024*1024;
type Result<T>=std::result::Result<T,&'static str>;
fn varint(mut n:u64,out:&mut Vec<u8>){while n>=128{out.push((n as u8&127)|128);n>>=7;}out.push(n as u8);}
fn number(field:u64,n:u64,out:&mut Vec<u8>){varint(field<<3,out);varint(n,out);}
fn bytes(field:u64,b:&[u8],out:&mut Vec<u8>){varint((field<<3)|2,out);varint(b.len() as u64,out);out.extend_from_slice(b);}
fn zig(n:i32)->u64{((n<<1)^(n>>31)) as u32 as u64}
fn command(id:u64,count:usize,out:&mut Vec<u8>){varint((count as u64)<<3|id,out);}
type Pt=(i32,i32);
fn quant(c:Coord<f64>)->Pt{(c.x.round().clamp(-BUFFER as f64,(EXTENT+BUFFER)as f64)as i32,c.y.round().clamp(-BUFFER as f64,(EXTENT+BUFFER)as f64)as i32)}
fn delta(p:Pt,last:&mut Pt,out:&mut Vec<u8>){varint(zig(p.0-last.0),out);varint(zig(p.1-last.1),out);*last=p;}
fn area(p:&[Pt])->i64{p.iter().zip(p.iter().cycle().skip(1)).take(p.len()).map(|(a,b)|i64::from(a.0)*i64::from(b.1)-i64::from(b.0)*i64::from(a.1)).sum()}
fn path(line:&LineString<f64>,ring:Option<bool>,last:&mut Pt,out:&mut Vec<u8>)->bool{
 let mut p:Vec<_>=line.0.iter().copied().map(quant).collect();p.dedup();
 if let Some(exterior)=ring{
   if p.first()==p.last(){p.pop();}
   let a=area(&p);if p.len()<3||a==0{return false}
   if (a>0)!=exterior{p.reverse();}
 }else if p.len()<2{return false}
 command(1,1,out);delta(p[0],last,out);command(2,p.len()-1,out);
 for q in &p[1..]{delta(*q,last,out);}
 if ring.is_some(){command(7,1,out);}true
}
fn lat(y:f64)->f64{(std::f64::consts::PI*(1.-2.*y)).sinh().atan().to_degrees()}
pub fn bounds(z:u8,x:u32,y:u32)->Result<[f64;4]>{
 if z>22||x>=1u32<<z||y>=1u32<<z{return Err("INVALID_TILE")}
 let n=(1u32<<z)as f64;let b=BUFFER as f64/EXTENT as f64;
 Ok([((x as f64-b)/n*360.-180.).max(-180.),lat((y as f64+1.+b)/n),((x as f64+1.+b)/n*360.-180.).min(180.),lat((y as f64-b)/n)])
}
fn geometry(g:Geometry<f64>,z:u8,x:u32,y:u32)->Result<(u64,Vec<u8>)>{
 let n=(1u32<<z)as f64;
 let g=g.map_coords(|c|{
   let r=c.y.clamp(-85.0511287798066,85.0511287798066).to_radians();
   Coord{x:((c.x+180.)/360.*n-x as f64)*EXTENT as f64,y:((1.-r.tan().asinh()/std::f64::consts::PI)/2.*n-y as f64)*EXTENT as f64}
 });
 let min=-f64::from(BUFFER);let max=f64::from(EXTENT+BUFFER);
 let rect:Polygon<f64>=Rect::new(Coord{x:min,y:min},Coord{x:max,y:max}).to_polygon();
 let mut last=(0,0);let mut out=Vec::new();
 let points=|ps:Vec<geo::Point<f64>>,last:&mut Pt,out:&mut Vec<u8>|{
   let p:Vec<_>=ps.into_iter().filter(|p|p.x()>=min&&p.x()<=max&&p.y()>=min&&p.y()<=max).map(|p|quant(p.0)).collect();
   if !p.is_empty(){command(1,p.len(),out);for q in p{delta(q,last,out)}}
 };
 let lines=|ls:MultiLineString<f64>,last:&mut Pt,out:&mut Vec<u8>|{for l in rect.clip(&ls,false).0{path(&l,None,last,out);}};
 let polys=|ps:geo::MultiPolygon<f64>,last:&mut Pt,out:&mut Vec<u8>|{for p in rect.intersection(&ps).0{
   if path(p.exterior(),Some(true),last,out){for h in p.interiors(){path(h,Some(false),last,out);}}
 }};
 let kind=match g{
  Geometry::Point(p)=>{points(vec![p],&mut last,&mut out);1},
  Geometry::MultiPoint(p)=>{points(p.0,&mut last,&mut out);1},
  Geometry::LineString(l)=>{lines(MultiLineString(vec![l]),&mut last,&mut out);2},
  Geometry::MultiLineString(l)=>{lines(l,&mut last,&mut out);2},
  Geometry::Polygon(p)=>{polys(geo::MultiPolygon(vec![p]),&mut last,&mut out);3},
  Geometry::MultiPolygon(p)=>{polys(p,&mut last,&mut out);3},
  _=>return Err("UNSUPPORTED_GEOMETRY")
 };Ok((kind,out))
}
pub fn unified_tile(store:&crate::Store,layer:uuid::Uuid,revision:u64,z:u8,x:u32,y:u32,cancel:&std::sync::atomic::AtomicBool)->Result<Vec<u8>>{
 named_tile(store,layer,revision,z,x,y,"local",cancel)
}
pub fn named_tile(store:&crate::Store,layer:uuid::Uuid,revision:u64,z:u8,x:u32,y:u32,name:&str,cancel:&std::sync::atomic::AtomicBool)->Result<Vec<u8>>{
 let bbox=bounds(z,x,y)?;let mut after=0;let mut items=Vec::new();let mut size=0usize;
 loop{
  if cancel.load(std::sync::atomic::Ordering::Acquire){return Err("CANCELLED")}
  let page=store.query(layer,bbox,after,200,revision).map_err(|e|if e==crate::Error::Conflict{"STALE_REVISION"}else{"STORAGE_UNAVAILABLE"})?;
  for r in page.rows{size=size.saturating_add(r.image.wkb.len());if size>16*1024*1024||items.len()>=MAX_FEATURES as usize{return Err("TILE_DENSITY_LIMIT")}items.push((r.id,r.image.wkb));}
  match page.next{Some(n) if n>after=>after=n,Some(_)=>return Err("INVALID_CURSOR"),None=>break}
 }
 encode_named(items,z,x,y,name,cancel)
}
pub fn encode(rows:impl IntoIterator<Item=(uuid::Uuid,Vec<u8>)>,z:u8,x:u32,y:u32,cancel:&std::sync::atomic::AtomicBool)->Result<Vec<u8>>{
 encode_named(rows,z,x,y,"local",cancel)
}
fn encode_named(rows:impl IntoIterator<Item=(uuid::Uuid,Vec<u8>)>,z:u8,x:u32,y:u32,name:&str,cancel:&std::sync::atomic::AtomicBool)->Result<Vec<u8>>{
 use std::sync::atomic::Ordering;
 if name.is_empty()||name.len()>64{return Err("INVALID_LAYER")}bounds(z,x,y)?;
 let mut layer=Vec::new();bytes(1,name.as_bytes(),&mut layer);number(15,2,&mut layer);number(5,EXTENT as u64,&mut layer);
 bytes(3,b"stable_id",&mut layer);
 let mut values=Vec::new();
 for (id,wkb) in rows{
   if cancel.load(Ordering::Acquire){return Err("CANCELLED")}
   // Shared validator rejects unsupported dimensions and invalid topology.
   let g=parse_xy_wkb(&wkb,&ValidationLimits::default()).map_err(|_|"INVALID_GEOMETRY")?.geometry;
   let (kind,geom)=std::panic::catch_unwind(std::panic::AssertUnwindSafe(||geometry(g,z,x,y))).map_err(|_|"INVALID_GEOMETRY")??;
   if geom.is_empty(){continue}
   let mut value=Vec::new();bytes(1,id.to_string().as_bytes(),&mut value);
   let mut tags=Vec::new();varint(0,&mut tags);varint(values.len() as u64,&mut tags);values.push(value);
   let mut feature=Vec::new();bytes(2,&tags,&mut feature);number(3,kind,&mut feature);bytes(4,&geom,&mut feature);bytes(2,&feature,&mut layer);
   if layer.len()>MAX_TILE{return Err("RESOURCE_LIMIT")}
 }
 if cancel.load(Ordering::Acquire){return Err("CANCELLED")}
 for v in values{bytes(4,&v,&mut layer);}
 let mut out=Vec::new();bytes(3,&layer,&mut out);
 if out.len()>MAX_TILE{return Err("RESOURCE_LIMIT")}Ok(out)
}
#[cfg(test)]
mod tests{
 use super::*;
 #[test]fn point_geometry(){let(k,b)=geometry(Geometry::Point(geo::Point::new(0.,0.)),0,0,0).unwrap();assert_eq!(k,1);assert_eq!(b,vec![9,128,32,128,32]);}
 #[test]fn rejects_invalid_xyz(){assert!(bounds(23,0,0).is_err());assert!(bounds(1,2,0).is_err());}
 #[test]fn clip_line_and_polygon(){let l=LineString::from(vec![(-170.,0.),(170.,0.)]);let(k,b)=geometry(Geometry::LineString(l),2,1,2).unwrap();assert_eq!(k,2);assert!(!b.is_empty());let p=Rect::new(Coord{x:-100.,y:-20.},Coord{x:0.,y:20.}).to_polygon();let(k,b)=geometry(Geometry::Polygon(p),2,1,2).unwrap();assert_eq!(k,3);assert_eq!(b.last(),Some(&15));}
}
