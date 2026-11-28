//! Local LAZ/COPC provider. Opens immutable private staging files per bounded read.
use std::{fs,io::Read,path::Path};
type Result<T>=std::result::Result<T,&'static str>;
const MAX:u64=512*1024*1024;
const POINTS:u64=2048;
pub struct Cloud{file:tempfile::NamedTempFile,count:u64,copc:bool}
impl Cloud{
 pub fn open(path:&Path,copc:bool)->Result<Self>{
  let m=fs::symlink_metadata(path).map_err(|_|"STORAGE_UNAVAILABLE")?;
  if !m.file_type().is_file()||m.len()>MAX{return Err("RESOURCE_LIMIT")}
  let mut file=tempfile::NamedTempFile::new().map_err(|_|"STORAGE_UNAVAILABLE")?;
  let mut src=fs::File::open(path).map_err(|_|"STORAGE_UNAVAILABLE")?.take(MAX+1);
  let size=std::io::copy(&mut src,file.as_file_mut()).map_err(|_|"STORAGE_UNAVAILABLE")?;
  if size!=m.len(){return Err("SOURCE_CHANGED")}
  file.as_file().sync_all().map_err(|_|"STORAGE_UNAVAILABLE")?;
  let reader=las::Reader::from_path(file.path()).map_err(|_|"INVALID_LAZ")?;
  let count=reader.header().number_of_points();
  if count==0||count>100_000_000{return Err("RESOURCE_LIMIT")}
  drop(reader);
  if copc{copc_rs::CopcReader::from_path(file.path()).map_err(|_|"INVALID_COPC")?;}
  Ok(Self{file,count,copc})
 }
 pub fn count(&self)->u64{self.count}
 pub fn chunk(&self,n:u32)->Result<Vec<u8>>{
  let offset=u64::from(n)*POINTS;if offset>=self.count{return Err("INVALID_CHUNK")}
  let mut r=las::Reader::from_path(self.file.path()).map_err(|_|"INVALID_LAZ")?;
  r.seek(offset).map_err(|_|"INVALID_LAZ")?;
  let mut out=Vec::with_capacity((POINTS*24)as usize);
  for _ in 0..(self.count-offset).min(POINTS){
    let p=r.read_point().map_err(|_|"INVALID_LAZ")?.ok_or("TRUNCATED_LAZ")?;
    append(&mut out,[p.x,p.y,p.z])?;
  }
  Ok(out)
 }
 pub fn query(&self,b:[f64;6],level:i32)->Result<Vec<u8>>{
  if !self.copc{return Err("UNSUPPORTED_SPATIAL_QUERY")}
  if !(0..=16).contains(&level)||b.iter().any(|v|!v.is_finite())||(0..3).any(|i|b[i]>b[i+3]){return Err("INVALID_QUERY")}
  use copc_rs::{Bounds,BoundsSelection,CopcReader,LodSelection,Vector};
  let mut r=CopcReader::from_path(self.file.path()).map_err(|_|"INVALID_COPC")?;
  let bounds=Bounds{min:Vector{x:b[0],y:b[1],z:b[2]},max:Vector{x:b[3],y:b[4],z:b[5]}};
  let points=r.points(LodSelection::Level(level),BoundsSelection::Within(bounds)).map_err(|_|"INVALID_COPC")?;
  let mut out=Vec::new();
  for p in points.take(POINTS as usize+1){
    if out.len()>=POINTS as usize*24{return Err("POINT_DENSITY_LIMIT")}
    append(&mut out,[p.x,p.y,p.z])?;
  }
  Ok(out)
 }
}
fn append(out:&mut Vec<u8>,p:[f64;3])->Result<()>{
 if p.iter().any(|v|!v.is_finite()){return Err("INVALID_POINT")}
 for n in p{out.extend(n.to_le_bytes())}Ok(())
}

#[cfg(test)]
mod tests{
 use super::*;
 #[test] fn laz_roundtrip_bounded_seek(){
   let dir=tempfile::tempdir().unwrap();let p=dir.path().join("fixture.laz");
   let mut writer=las::Writer::from_path(&p,las::Header::default()).unwrap();
   for i in 0..2050{writer.write_point(las::Point{x:i as f64,y:11.,z:5.,..Default::default()}).unwrap();}
   writer.close().unwrap();drop(writer);
   let cloud=Cloud::open(&p,false).unwrap();assert_eq!(cloud.count(),2050);
   let a=cloud.chunk(0).unwrap();assert_eq!(a.len(),2048*24);
   let b=cloud.chunk(1).unwrap();assert_eq!(b.len(),2*24);
   let x=f64::from_le_bytes(b[..8].try_into().unwrap());assert!((x-2048.).abs()<0.01);
   assert!(cloud.chunk(2).is_err());assert!(cloud.query([0.;6],0).is_err());
 }
}
