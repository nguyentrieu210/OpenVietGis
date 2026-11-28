//! Streaming validation of COLMAP binary little-endian vertex-only fused PLY.
//! Accepts the known float32 XYZ/normals and uchar RGB schemas, no arbitrary
//! list properties or faces. Validates bytes, finite positions and bounds.
use std::io::{BufRead,BufReader,Read};
#[derive(Debug)]pub enum Error{Invalid,Limit,Cancelled,Io(std::io::Error)}
impl From<std::io::Error> for Error{fn from(e:std::io::Error)->Self{Self::Io(e)}}
#[derive(Debug,Clone,Copy)]pub struct Policy{pub max_points:u64,pub max_bytes:u64,pub max_abs_coordinate:f64}
#[derive(Debug,Clone,Copy,PartialEq)]pub struct Summary{pub count:u64,pub min:[f64;3],pub max:[f64;3],pub normals:bool}
fn line(r:&mut impl BufRead,b:&mut Vec<u8>,remaining:&mut usize)->Result<(),Error>{
 b.clear();
 loop{
 let s=r.fill_buf()?;if s.is_empty(){return Err(Error::Invalid);}
 let n=s.iter().position(|v|*v==b'\n').map_or(s.len(),|i|i+1);
 if n>*remaining||b.len()+n>4096{return Err(Error::Limit);}
 b.extend_from_slice(&s[..n]);*remaining-=n;let end=s[n-1]==b'\n';r.consume(n);
 if end{return Ok(());}
 }
}
pub fn verify(reader:impl Read,p:Policy,cancelled:impl FnMut()->bool)->Result<Summary,Error>{
 visit(reader,p,cancelled,|_|Ok(()))
}
/// Visits verified rows in stream order. Caller must discard partial products on failure.
pub fn visit(reader:impl Read,p:Policy,mut cancelled:impl FnMut()->bool,mut point:impl FnMut([f64;3])->Result<(),Error>)->Result<Summary,Error>{
 if p.max_points==0||p.max_points>1_000_000_000||p.max_bytes<64||!p.max_abs_coordinate.is_finite()||p.max_abs_coordinate<=0.{return Err(Error::Invalid);}
 let mut r=BufReader::with_capacity(64*1024,reader);let mut b=Vec::new();let mut left=64*1024usize;
 line(&mut r,&mut b,&mut left)?;if b.strip_suffix(b"\n").map(|v|v.strip_suffix(b"\r").unwrap_or(v))!=Some(b"ply".as_slice()){return Err(Error::Invalid);}
 let(mut count,mut format,mut properties)=(None,false,Vec::new());
 loop{
 if cancelled(){return Err(Error::Cancelled);}
 line(&mut r,&mut b,&mut left)?;
 let s=std::str::from_utf8(&b).map_err(|_|Error::Invalid)?.trim();
 if s=="end_header"{break;}
 let words:Vec<_>=s.split_ascii_whitespace().collect();
 match words.as_slice(){
 ["comment",..]|["obj_info",..]=>{},
 ["format","binary_little_endian","1.0"] if !format&&count.is_none()=>format=true,
 ["element","vertex",n] if format&&count.is_none()=>{
 let n=n.parse::<u64>().map_err(|_|Error::Invalid)?;
 if n==0||n>p.max_points{return Err(Error::Limit);}count=Some(n);
 },
 ["property",kind,name] if count.is_some()&&properties.len()<9=>properties.push((kind.to_string(),name.to_string())),
 _=>return Err(Error::Invalid),
 }
 }
 let count=count.ok_or(Error::Invalid)?;
 let names:Vec<_>=properties.iter().map(|(k,n)|(k.as_str(),n.as_str())).collect();
 let short=[("float","x"),("float","y"),("float","z"),("uchar","red"),("uchar","green"),("uchar","blue")];
 let long=[("float","x"),("float","y"),("float","z"),("float","nx"),("float","ny"),("float","nz"),("uchar","red"),("uchar","green"),("uchar","blue")];
 let normals=if names==long{true}else if names==short{false}else{return Err(Error::Invalid);};
 let stride=if normals{27usize}else{15usize};
 let bytes=count.checked_mul(stride as u64).and_then(|n|n.checked_add((64*1024-left) as u64)).ok_or(Error::Limit)?;
 if bytes>p.max_bytes{return Err(Error::Limit);}
 let mut lo=[f64::INFINITY;3];let mut hi=[f64::NEG_INFINITY;3];let mut row=[0u8;27];
 for i in 0..count{
 if i%4096==0&&cancelled(){return Err(Error::Cancelled);}
 r.read_exact(&mut row[..stride])?;
 let mut xyz=[0.;3];
 for k in 0..if normals{6}else{3}{
 let j=k*4;let v=f64::from(f32::from_le_bytes([row[j],row[j+1],row[j+2],row[j+3]]));
 if !v.is_finite(){return Err(Error::Invalid);}
 if k<3{if v.abs()>p.max_abs_coordinate{return Err(Error::Invalid);}lo[k]=lo[k].min(v);hi[k]=hi[k].max(v);xyz[k]=v;}
 }
 point(xyz)?;
 }
 let mut tail=[0];if r.read(&mut tail)?!=0{return Err(Error::Invalid);}
 if cancelled(){return Err(Error::Cancelled);}
 Ok(Summary{count,min:lo,max:hi,normals})
}
#[cfg(test)]mod tests{
 use super::*;
 fn fixture()->Vec<u8>{let mut b=b"ply\nformat binary_little_endian 1.0\nelement vertex 2\nproperty float x\nproperty float y\nproperty float z\nproperty uchar red\nproperty uchar green\nproperty uchar blue\nend_header\n".to_vec();
 for xyz in [[1f32,2.,3.],[-1.,4.,5.]]{for x in xyz{b.extend(x.to_le_bytes());}b.extend([1,2,3]);}b}
 fn policy()->Policy{Policy{max_points:10,max_bytes:4096,max_abs_coordinate:100.}}
 #[test]fn validates_actual_rows_and_bounds(){let b=fixture();let s=verify(b.as_slice(),policy(),||false).unwrap();assert_eq!(s.count,2);assert_eq!(s.min,[-1.,2.,3.]);assert_eq!(s.max,[1.,4.,5.]);}
 #[test]fn rejects_truncated_trailing_and_nonfinite(){
 let mut b=fixture();assert!(verify(&b[..b.len()-1],policy(),||false).is_err());b.push(0);assert!(verify(b.as_slice(),policy(),||false).is_err());
 b.pop();let n=b.len()-30;b[n..n+4].copy_from_slice(&f32::NAN.to_le_bytes());assert!(verify(b.as_slice(),policy(),||false).is_err());}
}
