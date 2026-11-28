//! Baseline uncompressed RGBA GeoTIFF, one bounded tile per file.
//! Not a COG. Caller supplies a validated projected EPSG CRS with metre units.
use super::ortho::Grid;
#[derive(Debug,Clone,Copy,PartialEq,Eq)]
pub enum Error{Invalid,Limit}
fn u16le(out:&mut Vec<u8>,v:u16){out.extend(v.to_le_bytes());}
fn u32le(out:&mut Vec<u8>,v:u32){out.extend(v.to_le_bytes());}
fn entry(out:&mut Vec<u8>,tag:u16,kind:u16,count:u32,value:u32){
    u16le(out,tag);u16le(out,kind);u32le(out,count);u32le(out,value);
}
pub fn rgba(grid:Grid,epsg:u16,pixels:&[u8],max_bytes:u64)->Result<Vec<u8>,Error>{
    if grid.width==0||grid.height==0||!grid.gsd.is_finite()||grid.gsd<=0.||
        !grid.origin_x.is_finite()||!grid.origin_y.is_finite()||epsg==0||epsg>=32767{return Err(Error::Invalid);}
    let n=u64::from(grid.width).checked_mul(u64::from(grid.height)).and_then(|n|n.checked_mul(4)).ok_or(Error::Limit)?;
    if n!=pixels.len() as u64{return Err(Error::Invalid);}
    // IFD starts at 8; 14 tags, 4-byte next IFD, 2 bytes padding.
    const BITS:u32=184; const SCALE:u32=192;const TIE:u32=216;const KEYS:u32=264;const DATA:u32=304;
    let total=n.checked_add(u64::from(DATA)).ok_or(Error::Limit)?;
    if total>max_bytes||total>256*1024*1024||total>u32::MAX as u64{return Err(Error::Limit);}
    let mut out=Vec::with_capacity(total as usize);
    out.extend(b"II");u16le(&mut out,42);u32le(&mut out,8);u16le(&mut out,14);
    entry(&mut out,256,4,1,grid.width);
    entry(&mut out,257,4,1,grid.height);
    entry(&mut out,258,3,4,BITS);
    entry(&mut out,259,3,1,1);
    entry(&mut out,262,3,1,2);
    entry(&mut out,273,4,1,DATA);
    entry(&mut out,277,3,1,4);
    entry(&mut out,278,4,1,grid.height);
    entry(&mut out,279,4,1,n as u32);
    entry(&mut out,284,3,1,1);
    entry(&mut out,338,3,1,2);
    entry(&mut out,33550,12,3,SCALE);
    entry(&mut out,33922,12,6,TIE);
    entry(&mut out,34735,3,20,KEYS);
    u32le(&mut out,0);out.resize(BITS as usize,0);
    for v in [8u16;4]{u16le(&mut out,v);}
    for v in [grid.gsd,grid.gsd,0.0]{out.extend(v.to_le_bytes());}
    for v in [0.0,0.0,0.0,grid.origin_x,grid.origin_y,0.0]{out.extend(v.to_le_bytes());}
    for v in [1u16,1,0,4,1024,0,1,1,1025,0,1,1,3072,0,1,epsg,3076,0,1,9001]{u16le(&mut out,v);}
    debug_assert_eq!(out.len(),DATA as usize);
    out.extend_from_slice(pixels);Ok(out)
}
#[cfg(test)]mod tests{
 use super::*;
 fn grid()->Grid{Grid{origin_x:500000.,origin_y:1200000.,gsd:0.025,width:1,height:1}}
 #[test]fn baseline_tags_and_pixel_payload(){
 let p=rgba(grid(),32648,&[1,2,3,255],308).unwrap();
 assert_eq!(&p[..8],b"II\x2a\0\x08\0\0\0");assert_eq!(&p[304..],&[1,2,3,255]);
 assert_eq!(f64::from_le_bytes(p[240..248].try_into().unwrap()),500000.);
 assert_eq!(u16::from_le_bytes(p[294..296].try_into().unwrap()),32648);}
 #[test]fn refuses_bad_payload_and_budget(){
 assert_eq!(rgba(grid(),32648,&[1,2,3],1000),Err(Error::Invalid));
 assert_eq!(rgba(grid(),32648,&[1,2,3,255],307),Err(Error::Limit));}
}
