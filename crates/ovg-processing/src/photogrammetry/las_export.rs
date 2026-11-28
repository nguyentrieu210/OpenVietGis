//! Reference LAS 1.2 point format 2 encoder, projected metre coordinates.
//! Uncompressed LAS, not LAZ or COPC. CRS and vertical datum must be host qualified.
use super::depth::Point;
#[derive(Debug,Clone,Copy,PartialEq,Eq)]
pub enum Error{Invalid,Limit,CoordinateOverflow}
const HEADER:usize=227;
const VLR:usize=54+40;
const OFFSET:usize=HEADER+VLR;
const RECORD:usize=26;
fn put(b:&mut[u8],i:usize,v:&[u8]){b[i..i+v.len()].copy_from_slice(v);}
fn text(b:&mut[u8],i:usize,n:usize,s:&str){let len=n.min(s.len());put(b,i,&s.as_bytes()[..len]);}
fn quantize(x:f64,offset:f64,scale:f64)->Result<i32,Error>{
 let q=((x-offset)/scale).round();
 if !q.is_finite()||q<i32::MIN as f64||q>i32::MAX as f64{return Err(Error::CoordinateOverflow);}
 Ok(q as i32)
}
pub fn encode(points:&[Point],epsg:u16,scale:f64,offset:[f64;3],max_bytes:u64)->Result<Vec<u8>,Error>{
 if points.is_empty()||epsg==0||epsg>=32767||!scale.is_finite()||scale<=0.||
 !offset.iter().all(|x|x.is_finite()){return Err(Error::Invalid);}
 let len=points.len().checked_mul(RECORD).and_then(|n|n.checked_add(OFFSET)).ok_or(Error::Limit)?;
 if points.len()>5_000_000||len as u64>max_bytes||len>256*1024*1024{return Err(Error::Limit);}
 let mut out=vec![0u8;OFFSET];put(&mut out,0,b"LASF");
 out[24]=1;out[25]=2;text(&mut out,26,32,"OpenVietGIS");text(&mut out,58,32,"OVG Photogrammetry");
 put(&mut out,94,&(HEADER as u16).to_le_bytes());put(&mut out,96,&(OFFSET as u32).to_le_bytes());
 put(&mut out,100,&1u32.to_le_bytes());out[104]=2;put(&mut out,105,&(RECORD as u16).to_le_bytes());
 let count=points.len() as u32;put(&mut out,107,&count.to_le_bytes());put(&mut out,111,&count.to_le_bytes());
 for i in 0..3{put(&mut out,131+i*8,&scale.to_le_bytes());put(&mut out,155+i*8,&offset[i].to_le_bytes());}
 // GeoTIFF GeoKeyDirectoryTag VLR: projected, PixelIsPoint, EPSG, metres.
 text(&mut out,HEADER+2,16,"LASF_Projection");
 put(&mut out,HEADER+18,&34735u16.to_le_bytes());put(&mut out,HEADER+20,&40u16.to_le_bytes());
 text(&mut out,HEADER+22,32,"GeoKeyDirectoryTag");
 for(i,k)in [1u16,1,0,4,1024,0,1,1,1025,0,1,2,3072,0,1,epsg,3076,0,1,9001].into_iter().enumerate(){
 put(&mut out,HEADER+54+i*2,&k.to_le_bytes());}
 let mut lo=[f64::INFINITY;3];let mut hi=[f64::NEG_INFINITY;3];
 out.reserve(len-OFFSET);
 for p in points{
 if !p.xyz.iter().all(|v|v.is_finite())||!p.confidence.is_finite()||!(0.0..=1.0).contains(&p.confidence){return Err(Error::Invalid);}
 let mut row=[0u8;RECORD];
 for i in 0..3{
 let q=quantize(p.xyz[i],offset[i],scale)?;let v=f64::from(q)*scale+offset[i];if !v.is_finite(){return Err(Error::CoordinateOverflow);}
 lo[i]=lo[i].min(v);hi[i]=hi[i].max(v);put(&mut row,i*4,&q.to_le_bytes());}
 // Photogrammetric points are not LiDAR returns: single-return placeholder,
 // classification unclassified, intensity unavailable. Confidence is not intensity.
 row[14]=9;row[15]=1;
 if let Some(rgb)=p.rgb{for(i,v)in rgb.into_iter().enumerate(){put(&mut row,20+i*2,&(u16::from(v)*257).to_le_bytes());}}
 out.extend(row);
 }
 for i in 0..3{put(&mut out,179+i*16,&hi[i].to_le_bytes());put(&mut out,187+i*16,&lo[i].to_le_bytes());}
 Ok(out)
}
#[cfg(test)]mod tests{
 use super::*;
 fn point()->Point{Point{xyz:[500000.012,1200000.034,10.],rgb:Some([255,128,0]),confidence:1.}}
 #[test]fn header_quantization_and_color(){
 let b=encode(&[point()],32648,0.001,[500000.,1200000.,0.],1024).unwrap();
 assert_eq!(&b[..4],b"LASF");assert_eq!(b.len(),OFFSET+RECORD);
 assert_eq!(i32::from_le_bytes(b[OFFSET..OFFSET+4].try_into().unwrap()),12);
 assert_eq!(u16::from_le_bytes(b[OFFSET+20..OFFSET+22].try_into().unwrap()),65535);
 assert_eq!(u32::from_le_bytes(b[107..111].try_into().unwrap()),1);}
 #[test]fn rejects_overflow_and_budget(){
 assert_eq!(encode(&[point()],32648,1e-9,[0.;3],1024),Err(Error::CoordinateOverflow));
 assert_eq!(encode(&[point()],32648,0.001,[0.;3],100),Err(Error::Limit));}
}
