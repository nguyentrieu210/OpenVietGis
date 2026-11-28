//! Restricted remote tiled TIFF reader: classic/BigTIFF little endian, 8-bit grayscale,
//! uncompressed or Deflate; bounded IFD/overview traversal and decoding.
use std::io::{self,Read,Seek,SeekFrom};
use serde::Serialize;
fn bad()->io::Error{io::Error::new(io::ErrorKind::InvalidData,"UNSUPPORTED_REMOTE_TIFF_PROFILE")}
fn u16le(b:&[u8])->u16{u16::from_le_bytes([b[0],b[1]])}
fn u32le(b:&[u8])->u32{u32::from_le_bytes([b[0],b[1],b[2],b[3]])}
fn u64le(b:&[u8])->u64{u64::from_le_bytes(b[..8].try_into().unwrap())}
fn at<R:Read+Seek>(r:&mut R,len:u64,off:u64,out:&mut [u8])->io::Result<()>{
 if off.checked_add(out.len()as u64).is_none_or(|n|n>len){return Err(bad())}
 r.seek(SeekFrom::Start(off))?;r.read_exact(out)
}
#[derive(Clone,Debug,Serialize)]
pub struct Info{pub width:u32,pub height:u32,pub tile_width:u32,pub tile_height:u32,pub columns:u32,pub rows:u32,pub epsg:Option<u32>,pub geotransform:Option<[f64;6]>,pub compression:u32,pub level:u32}
#[derive(Clone)]
struct Tag{kind:u16,count:u64,raw:Vec<u8>}
impl Tag{
 fn data<R:Read+Seek>(&self,r:&mut R,len:u64,size:usize,max:usize)->io::Result<Vec<u8>>{
  let n=usize::try_from(self.count).ok().and_then(|n|n.checked_mul(size)).filter(|&n|n>0&&n<=max).ok_or_else(bad)?;
  if n<=self.raw.len(){return Ok(self.raw[..n].to_vec())}
  let mut out=vec![0;n];at(r,len,if self.raw.len()==8{u64le(&self.raw)}else{u32le(&self.raw)as u64},&mut out)?;Ok(out)
 }
 fn integers<R:Read+Seek>(&self,r:&mut R,len:u64,max:usize)->io::Result<Vec<u64>>{
  let size=match self.kind{3=>2,4|13=>4,16|18=>8,_=>return Err(bad())};
  let b=self.data(r,len,size,max)?;
  Ok(b.chunks_exact(size).map(|v|match size{2=>u16le(v)as u64,4=>u32le(v)as u64,_=>u64le(v)}).collect())
 }
 fn doubles<R:Read+Seek>(&self,r:&mut R,len:u64,n:usize)->io::Result<Vec<f64>>{
  if self.kind!=12||self.count as usize!=n{return Err(bad())}
  let b=self.data(r,len,8,128)?;
  let vals:Vec<_>=b.chunks_exact(8).map(|v|f64::from_le_bytes(v.try_into().unwrap())).collect();
  if vals.iter().any(|v|!v.is_finite()){return Err(bad())}Ok(vals)
 }
}
pub struct TiledTiff{pub info:Info,offsets:Vec<u64>,counts:Vec<u64>,white_zero:bool,len:u64,predictor:u32,next:u64,sub:Vec<u64>,reduced:bool}
impl TiledTiff{
 pub fn open<R:Read+Seek>(r:&mut R,len:u64)->io::Result<Self>{
  let (big,off)=Self::header(r,len)?;
  Self::directory(r,len,big,off,0)
 }
 fn header<R:Read+Seek>(r:&mut R,len:u64)->io::Result<(bool,u64)>{
  let mut h=[0;8];at(r,len,0,&mut h)?;
  match &h[..4]{
   b"II\x2a\x00"=>Ok((false,u32le(&h[4..])as u64)),
   b"II\x2b\x00" if h[4..]==[8,0,0,0]=>{let mut p=[0;8];at(r,len,8,&mut p)?;Ok((true,u64le(&p)))},
   _=>Err(bad())
  }
 }
 fn directory<R:Read+Seek>(r:&mut R,len:u64,big:bool,off:u64,level:u32)->io::Result<Self>{
  if off==0{return Err(bad())}
  let (head,stride,tail)=if big{(8usize,20usize,8usize)}else{(2,12,4)};
  let mut n=vec![0;head];at(r,len,off,&mut n)?;
  let n=if big{u64le(&n)}else{u16le(&n)as u64};
  if n==0||n>256{return Err(bad())}
  let start=off.checked_add(head as u64).ok_or_else(bad)?;
  let mut entries=vec![0;n as usize*stride];at(r,len,start,&mut entries)?;
  let mut next=vec![0;tail];at(r,len,start.checked_add(entries.len()as u64).ok_or_else(bad)?,&mut next)?;
  let next=if big{u64le(&next)}else{u32le(&next)as u64};
  let mut tags=std::collections::BTreeMap::new();
  for e in entries.chunks_exact(stride){
   let tag=u16le(e);let v=Tag{kind:u16le(&e[2..]),count:if big{u64le(&e[4..])}else{u32le(&e[4..])as u64},raw:if big{e[12..20].to_vec()}else{e[8..12].to_vec()}};
   if tags.insert(tag,v).is_some(){return Err(bad())}
  }
  let scalar=|r:&mut R,key:u16,default:Option<u32>|->io::Result<u32>{
   match tags.get(&key){None=>default.ok_or_else(bad),Some(t) if t.count==1=>u32::try_from(t.integers(r,len,8)?[0]).map_err(|_|bad()),_=>Err(bad())}
  };
  let width=scalar(r,256,None)?;let height=scalar(r,257,None)?;
  let tw=scalar(r,322,None)?;let th=scalar(r,323,None)?;
  let photo=scalar(r,262,None)?;let compression=scalar(r,259,Some(1))?;let predictor=scalar(r,317,Some(1))?;
  let reduced=scalar(r,254,Some(0))?==1;
  let sub=match tags.get(&330){Some(t)=>t.integers(r,len,128)?,None=>vec![]};
  if width==0||height==0||width>10_000_000||height>10_000_000||tw==0||th==0||tw>1024||th>1024||
   !matches!(photo,0|1)||scalar(r,258,None)?!=8||!matches!(compression,1|8|32946)||!matches!(predictor,1|2)||scalar(r,277,Some(1))?!=1||
   scalar(r,284,Some(1))?!=1||scalar(r,339,Some(1))?!=1||scalar(r,274,Some(1))?!=1||tags.contains_key(&338){return Err(bad())}
  let columns=width.div_ceil(tw);let rows=height.div_ceil(th);
  let count=columns.checked_mul(rows).filter(|n|*n<=262_144).ok_or_else(bad)? as usize;
  let offsets=tags.get(&324).ok_or_else(bad)?.integers(r,len,2*1024*1024)?;
  let counts=tags.get(&325).ok_or_else(bad)?.integers(r,len,2*1024*1024)?;
  if offsets.len()!=count||counts.len()!=count||offsets.iter().zip(&counts).any(|(&o,&n)|o==0||n==0||n>2*1024*1024||(compression==1&&n!=u64::from(tw*th))||o.checked_add(n).is_none_or(|e|e>len)){return Err(bad())}
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
  Ok(Self{info:Info{width,height,tile_width:tw,tile_height:th,columns,rows,epsg,geotransform,compression,level},offsets,counts,white_zero:photo==0,len,predictor,next,sub,reduced})
 }
 /// Traverse at most 12 directories, rejecting cycles. Only compatible reduced images.
 pub fn pyramid<R:Read+Seek>(r:&mut R,len:u64)->io::Result<Vec<Self>>{
  let (big,root)=Self::header(r,len)?;
  let first=Self::directory(r,len,big,root,0)?;
  let mut todo=std::collections::VecDeque::new();todo.extend(first.sub.iter().copied());if first.next!=0{todo.push_back(first.next)}
  let mut seen=std::collections::HashSet::from([root]);let mut out=vec![first];
  while let Some(off)=todo.pop_front(){
   if seen.len()>=12||off==0||!seen.insert(off){return Err(bad())}
   let mut d=Self::directory(r,len,big,off,0)?;
   todo.extend(d.sub.iter().copied());if d.next!=0{todo.push_back(d.next)}
   let base=&out[0].info;
   if !d.reduced||d.info.width>=base.width||d.info.height>=base.height{return Err(bad())}
   if d.info.geotransform.is_none(){if let Some(mut g)=base.geotransform{g[1]*=base.width as f64/d.info.width as f64;g[5]*=base.height as f64/d.info.height as f64;d.info.geotransform=Some(g);d.info.epsg=base.epsg;}}
   out.push(d);
  }
  out[1..].sort_by_key(|d|std::cmp::Reverse(u64::from(d.info.width)*u64::from(d.info.height)));
  for (i,d) in out.iter_mut().enumerate(){d.info.level=i as u32;}
  Ok(out)
 }
 pub fn tile<R:Read+Seek>(&self,r:&mut R,col:u32,row:u32)->io::Result<Vec<u8>>{
  if col>=self.info.columns||row>=self.info.rows{return Err(bad())}
  let i=(row*self.info.columns+col)as usize;
  let mut data=vec![0;self.counts[i]as usize];at(r,self.len,self.offsets[i],&mut data)?;
  let expected=(self.info.tile_width*self.info.tile_height)as usize;
  if self.info.compression!=1{
   let mut decoded=Vec::with_capacity(expected);let mut z=flate2::read::ZlibDecoder::new(data.as_slice());
   (&mut z).take(expected as u64+1).read_to_end(&mut decoded)?;
   if decoded.len()!=expected||z.total_in()!=data.len()as u64{return Err(bad())}data=decoded;
  }
  if data.len()!=expected{return Err(bad())}
  if self.predictor==2{for row in data.chunks_exact_mut(self.info.tile_width as usize){for i in 1..row.len(){row[i]=row[i].wrapping_add(row[i-1]);}}}
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
