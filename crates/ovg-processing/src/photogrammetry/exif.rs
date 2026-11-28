//! Bounded JPEG APP1 EXIF/TIFF metadata reader. GPS is a position hint only;
//! altitude datum, RTK fix, uncertainty and PPK must not be inferred.
#[derive(Debug,Clone,Copy,PartialEq)]
pub struct Gps{pub latitude:f64,pub longitude:f64,pub altitude_m:Option<f64>}
#[derive(Debug,Clone,Copy,PartialEq)]
pub struct Metadata{pub orientation:Option<u16>,pub gps:Option<Gps>}
#[derive(Debug,Clone,Copy,PartialEq,Eq)]
pub enum Error{Invalid,Truncated,Limit}
struct Tiff<'a>{b:&'a[u8],le:bool}
impl<'a>Tiff<'a>{
 fn bytes(&self,off:usize,n:usize)->Result<&'a[u8],Error>{self.b.get(off..off.checked_add(n).ok_or(Error::Limit)?).ok_or(Error::Truncated)}
 fn u16(&self,off:usize)->Result<u16,Error>{let a=self.bytes(off,2)?.try_into().map_err(|_|Error::Invalid)?;
 Ok(if self.le{u16::from_le_bytes(a)}else{u16::from_be_bytes(a)})}
 fn u32(&self,off:usize)->Result<u32,Error>{let a=self.bytes(off,4)?.try_into().map_err(|_|Error::Invalid)?;
 Ok(if self.le{u32::from_le_bytes(a)}else{u32::from_be_bytes(a)})}
 fn entries(&self,off:usize)->Result<Vec<(u16,usize)>,Error>{
 let n=self.u16(off)? as usize;if n>512{return Err(Error::Limit);}
 self.bytes(off.checked_add(2).ok_or(Error::Limit)?,n*12+4)?;
 let mut out=Vec::new();let mut seen=std::collections::BTreeSet::new();
 for i in 0..n{let p=off+2+i*12;let tag=self.u16(p)?;if !seen.insert(tag){return Err(Error::Invalid);}out.push((tag,p));}Ok(out)}
 fn field(&self,p:usize,kind:u16,count:u32)->Result<usize,Error>{
 if self.u16(p+2)?!=kind||self.u32(p+4)?!=count{return Err(Error::Invalid);}
 let size=match kind{1|2=>1usize,3=>2,4=>4,5=>8,_=>return Err(Error::Invalid)};
 let len=size.checked_mul(count as usize).ok_or(Error::Limit)?;
 let off=if len<=4{p+8}else{self.u32(p+8)? as usize};self.bytes(off,len)?;Ok(off)}
 fn rational(&self,off:usize)->Result<f64,Error>{let n=self.u32(off)?;let d=self.u32(off+4)?;if d==0{return Err(Error::Invalid);}Ok(f64::from(n)/f64::from(d))}
 fn coord(&self,p:usize,max:f64)->Result<f64,Error>{let off=self.field(p,5,3)?;
 let d=self.rational(off)?;let m=self.rational(off+8)?;let s=self.rational(off+16)?;
 let value=d+m/60.+s/3600.;if d>max||m>=60.||s>=60.||value>max{return Err(Error::Invalid);}Ok(value)}
}
pub fn parse_tiff(bytes:&[u8])->Result<Metadata,Error>{
 if bytes.len()>65533{return Err(Error::Limit);}
 let le=match bytes.get(..2){Some(b"II")=>true,Some(b"MM")=>false,_=>return Err(Error::Invalid)};
 let t=Tiff{b:bytes,le};if t.u16(2)?!=42{return Err(Error::Invalid);}
 let off=t.u32(4)? as usize;if off<8{return Err(Error::Invalid);}
 let mut out=Metadata{orientation:None,gps:None};let mut gps=None;
 for(tag,p)in t.entries(off)?{match tag{
 0x0112=>{let v=t.u16(t.field(p,3,1)?)?;if !(1..=8).contains(&v){return Err(Error::Invalid);}out.orientation=Some(v);}
 0x8825=>gps=Some(t.u32(t.field(p,4,1)?)? as usize),_=>{}}}
 if let Some(g)=gps{
 if g<8||g==off{return Err(Error::Invalid);}
 let(mut lat,mut lon,mut ns,mut ew,mut alt,mut alt_ref)=(None,None,None,None,None,None);
 for(tag,p)in t.entries(g)?{match tag{
 1=>{let b=t.bytes(t.field(p,2,2)?,2)?;if b[1]!=0||!matches!(b[0],b'N'|b'S'){return Err(Error::Invalid);}ns=Some(b[0]);}
 2=>lat=Some(t.coord(p,90.)?),
 3=>{let b=t.bytes(t.field(p,2,2)?,2)?;if b[1]!=0||!matches!(b[0],b'E'|b'W'){return Err(Error::Invalid);}ew=Some(b[0]);}
 4=>lon=Some(t.coord(p,180.)?),
 5=>{let v=t.bytes(t.field(p,1,1)?,1)?[0];if v>1{return Err(Error::Invalid);}alt_ref=Some(v);}
 6=>alt=Some(t.rational(t.field(p,5,1)?)?),_=>{}}}
 if let (Some(lat),Some(lon),Some(ns),Some(ew))=(lat,lon,ns,ew){
 out.gps=Some(Gps{latitude:if ns==b'S'{-lat}else{lat},longitude:if ew==b'W'{-lon}else{lon},
 altitude_m:alt.zip(alt_ref).map(|(a,r)|if r==1{-a}else{a})});}
 }
 Ok(out)
}
/// Search only the bounded prefix before Start of Scan. Missing EXIF is normal.
pub fn jpeg(bytes:&[u8],max_header:usize)->Result<Option<Metadata>,Error>{
 if max_header==0||max_header>4*1024*1024{return Err(Error::Limit);}
 let b=&bytes[..bytes.len().min(max_header)];if b.get(..2)!=Some(&[0xff,0xd8][..]){return Err(Error::Invalid);}
 let mut p=2usize;
 while p<b.len(){if b[p]!=0xff{return Err(Error::Invalid);}
 while b.get(p)==Some(&0xff){p+=1;}
 let marker=*b.get(p).ok_or(Error::Truncated)?;p+=1;
 if marker==0xda||marker==0xd9{return Ok(None);}
 if marker==0x01{continue;}if marker==0||marker==0xd8||(0xd0..=0xd7).contains(&marker){return Err(Error::Invalid);}
 let len=b.get(p..p+2).ok_or(Error::Truncated)?;
 let n=u16::from_be_bytes([len[0],len[1]]) as usize;if n<2{return Err(Error::Invalid);}
 let end=p.checked_add(n).ok_or(Error::Limit)?;let seg=b.get(p+2..end).ok_or(Error::Truncated)?;
 if marker==0xe1&&seg.starts_with(b"Exif\0\0"){return parse_tiff(&seg[6..]).map(Some);}p=end;
 }
 Err(Error::Truncated)
}
#[cfg(test)]mod tests{
 use super::*;
 fn orientation()->Vec<u8>{vec![b'I',b'I',42,0,8,0,0,0,1,0,0x12,1,3,0,1,0,0,0,6,0,0,0,0,0,0,0]}
 #[test]fn orientation_and_jpeg_app1(){let t=orientation();assert_eq!(parse_tiff(&t).unwrap().orientation,Some(6));
 let mut j=vec![0xff,0xd8,0xff,0xe1];j.extend(((t.len()+8) as u16).to_be_bytes());j.extend(b"Exif\0\0");j.extend(t);
 assert_eq!(jpeg(&j,1024).unwrap().unwrap().orientation,Some(6));}
 #[test]fn missing_and_truncated(){assert_eq!(jpeg(&[0xff,0xd8,0xff,0xda],100),Ok(None));assert_eq!(parse_tiff(&orientation()[..10]),Err(Error::Truncated));}
 #[test]fn gps_signed_coordinates(){
 let mut b=vec![0u8;160];b[..8].copy_from_slice(b"II\x2a\0\x08\0\0\0");
 fn w16(b:&mut[u8],p:usize,n:u16){b[p..p+2].copy_from_slice(&n.to_le_bytes());}
 fn w32(b:&mut[u8],p:usize,n:u32){b[p..p+4].copy_from_slice(&n.to_le_bytes());}
 fn e(b:&mut[u8],p:usize,tag:u16,kind:u16,n:u32,v:u32){w16(b,p,tag);w16(b,p+2,kind);w32(b,p+4,n);w32(b,p+8,v);}
 w16(&mut b,8,1);e(&mut b,10,0x8825,4,1,26);w16(&mut b,26,4);
 e(&mut b,28,1,2,2,u32::from(b'S'));e(&mut b,40,2,5,3,80);
 e(&mut b,52,3,2,2,u32::from(b'E'));e(&mut b,64,4,5,3,104);
 for (i,n) in [10,30,0,106,15,0].into_iter().enumerate(){w32(&mut b,80+i*8,n);w32(&mut b,84+i*8,1);}
 let g=parse_tiff(&b).unwrap().gps.unwrap();assert_eq!(g.latitude,-10.5);assert_eq!(g.longitude,106.25);assert_eq!(g.altitude_m,None);
 }
}
