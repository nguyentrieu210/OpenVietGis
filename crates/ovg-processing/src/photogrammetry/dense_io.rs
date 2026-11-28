//! COLMAP dense depth map format: ASCII width&height&channels& followed by
//! little-endian float32 data. Scalar depth only; normal maps are rejected.
use std::io::{Read,Seek,SeekFrom};
use super::tiles::Window;
#[derive(Debug)]pub enum Error{Io(std::io::Error),Invalid,Limit}
impl From<std::io::Error> for Error{fn from(e:std::io::Error)->Self{Self::Io(e)}}
#[derive(Debug,Clone,Copy)]pub struct Header{pub width:u32,pub height:u32,pub offset:u64}
pub struct DepthReader<R>{reader:R,header:Header}
fn field(r:&mut impl Read)->Result<u32,Error>{
 let mut n=0u32;let mut digits=0;
 loop{let mut b=[0];r.read_exact(&mut b)?;
 if b[0]==b'&'{return if digits>0&&n>0{Ok(n)}else{Err(Error::Invalid)};}
 if !b[0].is_ascii_digit()||digits>=10{return Err(Error::Invalid);}
 n=n.checked_mul(10).and_then(|n|n.checked_add(u32::from(b[0]-b'0'))).ok_or(Error::Limit)?;digits+=1;
 }
}
impl<R:Read+Seek> DepthReader<R>{
 pub fn open(mut reader:R,max_pixels:u64)->Result<Self,Error>{
 reader.seek(SeekFrom::Start(0))?;
 let width=field(&mut reader)?;let height=field(&mut reader)?;let channels=field(&mut reader)?;
 if channels!=1{return Err(Error::Invalid);}
 let n=u64::from(width)*u64::from(height);
 if n>max_pixels||n>1_000_000_000{return Err(Error::Limit);}
 let offset=reader.stream_position()?;
 let expected=offset.checked_add(n.checked_mul(4).ok_or(Error::Limit)?).ok_or(Error::Limit)?;
 if reader.seek(SeekFrom::End(0))?!=expected{return Err(Error::Invalid);}
 Ok(Self{reader,header:Header{width,height,offset}})
 }
 pub fn header(&self)->Header{self.header}
 pub fn window(&mut self,w:Window,max_bytes:u64)->Result<Vec<f32>,Error>{
 let h=self.header;
 if w.width==0||w.height==0||w.x.checked_add(w.width).is_none_or(|x|x>h.width)||
 w.y.checked_add(w.height).is_none_or(|y|y>h.height){return Err(Error::Invalid);}
 let n=u64::from(w.width)*u64::from(w.height);
 let bytes=n.checked_mul(4).ok_or(Error::Limit)?;
 let row_bytes=u64::from(w.width)*4;
 if bytes.checked_add(row_bytes).ok_or(Error::Limit)?>max_bytes||bytes>256*1024*1024{return Err(Error::Limit);}
 let mut out=Vec::with_capacity(n as usize);let mut row=vec![0u8;row_bytes as usize];
 for y in w.y..w.y+w.height{
 let pos=h.offset+(u64::from(y)*u64::from(h.width)+u64::from(w.x))*4;
 self.reader.seek(SeekFrom::Start(pos))?;self.reader.read_exact(&mut row)?;
 out.extend(row.chunks_exact(4).map(|b|f32::from_le_bytes([b[0],b[1],b[2],b[3]])));
 }
 Ok(out)
 }
}
#[cfg(test)]mod tests{
 use super::*;use std::io::Cursor;
 fn fixture()->Vec<u8>{let mut b=b"3&2&1&".to_vec();for x in [1f32,2.,3.,4.,5.,6.]{b.extend(x.to_le_bytes());}b}
 #[test]fn reads_region_without_whole_depth_map(){
 let mut d=DepthReader::open(Cursor::new(fixture()),6).unwrap();
 assert_eq!(d.window(Window{x:1,y:0,width:2,height:2},24).unwrap(),vec![2.,3.,5.,6.]);}
 #[test]fn rejects_truncation_and_budget(){
 let mut b=fixture();b.pop();assert!(matches!(DepthReader::open(Cursor::new(b),6),Err(Error::Invalid)));
 let mut d=DepthReader::open(Cursor::new(fixture()),6).unwrap();
 assert!(matches!(d.window(Window{x:0,y:0,width:3,height:2},24),Err(Error::Limit)));}
}
