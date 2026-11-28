//! Restricted remote tiled TIFF reader. Classic TIFF, little endian, unsigned
//! 8-bit grayscale, uncompressed contiguous tiles. No full-file fallback.
use std::io::{self,Read,Seek,SeekFrom};
use serde::Serialize;
fn bad()->io::Error{io::Error::new(io::ErrorKind::InvalidData,"UNSUPPORTED_REMOTE_TIFF_PROFILE")}
fn u16le(b:&[u8])->u16{u16::from_le_bytes([b[0],b[1]])}
fn u32le(b:&[u8])->u32{u32::from_le_bytes([b[0],b[1],b[2],b[3]])}
fn at<R:Read+Seek>(r:&mut R,len:u64,off:u64,out:&mut [u8])->io::Result<()>{
 if off.checked_add(out.len()as u64).is_none_or(|n|n>len){return Err(bad())}
 r.seek(SeekFrom::Start(off))?;r.read_exact(out)
}
#[derive(Clone,Debug,Serialize)]
pub struct Info{pub width:u32,pub height:u32,pub tile_width:u32,pub tile_height:u32,pub columns:u32,pub rows:u32,pub epsg:Option<u32>,pub geotransform:Option<[f64;6]>}
#[derive(Clone)]
struct Tag{kind:u16,count:u32,raw:[u8;4]}
impl Tag{
 fn data<R:Read+Seek>(&self,r:&mut R,len:u64,size:usize,max:usize)->io::Result<Vec<u8>>{
  let n=(self.count as usize).checked_mul(size).filter(|&n|n>0&&n<=max).ok_or_else(bad)?;
  if n<=4{return Ok(self.raw[..n].to_vec())}
  let mut out=vec![0;n];at(r,len,u32le(&self.raw)as u64,&mut out)?;Ok(out)
 }
 fn integers<R:Read+Seek>(&self,r:&mut R,len:u64,max:usize)->io::Result<Vec<u32>>{
  let size=match self.kind{3=>2,4=>4,_=>return Err(bad())};
  let b=self.data(r,len,size,max)?;
  Ok(b.chunks_exact(size).map(|v|if size==2{u16le(v)as u32}else{u32le(v)}).collect())
 }
 fn doubles<R:Read+Seek>(&self,r:&mut R,len:u64,n:usize)->io::Result<Vec<f64>>{
  if self.kind!=12||self.count as usize!=n{return Err(bad())}
  let b=self.data(r,len,8,128)?;
  let vals:Vec<_>=b.chunks_exact(8).map(|v|f64::from_le_bytes(v.try_into().unwrap())).collect();
  if vals.iter().any(|v|!v.is_finite()){return Err(bad())}Ok(vals)
 }
}
pub struct TiledTiff{pub info:Info,offsets:Vec<u32>,counts:Vec<u32>,white_zero:bool,len:u64}
impl TiledTiff{
 pub fn open<R:Read+Seek>(r:&mut R,len:u64)->io::Result<Self>{
  let mut h=[0;8];at(r,len,0,&mut h)?;
  if &h[..4]!=b"II\x2a\x00"{return Err(bad())}
  let off=u32le(&h[4..])as u64;
  let mut n=[0;2];at(r,len,off,&mut n)?;let n=u16le(&n)as usize;
  if n==0||n>256{return Err(bad())}
  let mut entries=vec![0;n*12];at(r,len,off+2,&mut entries)?;
  let mut tags=std::collections::BTreeMap::new();
  for e in entries.chunks_exact(12){
   let tag=u16le(e);let v=Tag{kind:u16le(&e[2..]),count:u32le(&e[4..]),raw:e[8..12].try_into().unwrap()};
   if tags.insert(tag,v).is_some(){return Err(bad())}
  }
  let scalar=|r:&mut R,key:u16,default:Option<u32>|->io::Result<u32>{
   match tags.get(&key){None=>default.ok_or_else(bad),Some(t) if t.count==1=>Ok(t.integers(r,len,4)?[0]),_=>Err(bad())}
  };
  let width=scalar(r,256,None)?;let height=scalar(r,257,None)?;
  let tw=scalar(r,322,None)?;let th=scalar(r,323,None)?;
  let photo=scalar(r,262,None)?;
  if width==0||height==0||width>10_000_000||height>10_000_000||tw==0||th==0||tw>1024||th>1024||
   !matches!(photo,0|1)||scalar(r,258,None)?!=8||scalar(r,259,Some(1))?!=1||scalar(r,277,Some(1))?!=1||
   scalar(r,284,Some(1))?!=1||scalar(r,339,Some(1))?!=1||scalar(r,274,Some(1))?!=1||tags.contains_key(&338){return Err(bad())}
  let columns=width.div_ceil(tw);let rows=height.div_ceil(th);
  let count=columns.checked_mul(rows).filter(|n|*n<=262_144).ok_or_else(bad)? as usize;
  let offsets=tags.get(&324).ok_or_else(bad)?.integers(r,len,1024*1024)?;
  let counts=tags.get(&325).ok_or_else(bad)?.integers(r,len,1024*1024)?;
  if offsets.len()!=count||counts.len()!=count||offsets.iter().zip(&counts).any(|(&o,&n)|o==0||n!=tw*th||u64::from(o)+u64::from(n)>len){return Err(bad())}
  // GeoTIFF scale/tiepoint and inline EPSG keys, only PixelIsArea north-up.
  let mut epsg=None;let mut geotransform=None;
  if let (Some(scale),Some(tie),Some(keys))=(tags.get(&33550),tags.get(&33922),tags.get(&34735)){
   let scale=scale.doubles(r,len,3)?;let tie=tie.doubles(r,len,6)?;let k=keys.integers(r,len,4096)?;
   if k.len()>=4&&k[0]==1&&k.len()==4+4*k[3]as usize&&scale[0]>0.&&scale[1]>0.&&!tags.contains_key(&34264){
    let key=|id|k[4..].chunks_exact(4).find(|v|v[0]==id&&v[1]==0&&v[2]==1).map(|v|v[3]);
    if key(1025)==Some(1){
     epsg=match (key(1024),key(2048),key(3072)){(Some(2),Some(4326),_)=>Some(4326),(Some(1),_,Some(3857))=>Some(3857),_=>None};
     if epsg.is_some(){let gt=[tie[3]-tie[0]*scale[0],scale[0],0.,tie[4]+tie[1]*scale[1],0.,-scale[1]];if gt.iter().all(|v|v.is_finite()){geotransform=Some(gt)}}
    }
   }
  }
  Ok(Self{info:Info{width,height,tile_width:tw,tile_height:th,columns,rows,epsg,geotransform},offsets,counts,white_zero:photo==0,len})
 }
 pub fn tile<R:Read+Seek>(&self,r:&mut R,col:u32,row:u32)->io::Result<Vec<u8>>{
  if col>=self.info.columns||row>=self.info.rows{return Err(bad())}
  let i=(row*self.info.columns+col)as usize;
  let mut data=vec![0;self.counts[i]as usize];at(r,self.len,self.offsets[i]as u64,&mut data)?;
  if self.white_zero{for v in &mut data{*v=255-*v}}Ok(data)
 }
}
#[cfg(test)]mod tests{
 use super::*;
 fn fixture()->Vec<u8>{
  let tags=[(256,4,2),(257,4,2),(258,3,8),(259,3,1),(262,3,1),(277,3,1),(322,4,2),(323,4,2),(324,4,256),(325,4,4)];
  let mut b=b"II\x2a\x00\x08\x00\x00\x00".to_vec();b.extend((tags.len()as u16).to_le_bytes());
  for (tag,kind,value) in tags{b.extend((tag as u16).to_le_bytes());b.extend((kind as u16).to_le_bytes());b.extend(1u32.to_le_bytes());b.extend((value as u32).to_le_bytes());}
  b.resize(256,0);b.extend([1,2,3,4]);b
 }
 #[test]fn bounded_tile(){let b=fixture();let mut r=io::Cursor::new(&b);let t=TiledTiff::open(&mut r,b.len()as u64).unwrap();assert_eq!(t.tile(&mut r,0,0).unwrap(),vec![1,2,3,4]);assert!(t.tile(&mut r,1,0).is_err());}
 #[test]fn truncated_or_bad_offset(){let b=fixture();assert!(TiledTiff::open(&mut io::Cursor::new(&b[..258]),258).is_err());}
}
