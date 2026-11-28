//! Native JPEG decoding, bounded before pixel allocation. No EXIF orientation
//! is applied: the camera model and raster must share the original pixel frame.
use super::images::{self,Dimensions,Limits};
use jpeg_decoder::{Decoder,PixelFormat};
use std::io::Cursor;
#[derive(Debug)]pub enum Error{Invalid,Limit,Unsupported,Header(images::Error),Decode(jpeg_decoder::Error)}
pub struct Rgb{pub dimensions:Dimensions,pub pixels:Vec<u8>}
pub fn decode(bytes:&[u8],limits:Limits,max_work_bytes:u64)->Result<Rgb,Error>{
 if bytes.is_empty()||bytes.len()>256*1024*1024||max_work_bytes==0{return Err(Error::Limit);}
 let dimensions=images::jpeg_dimensions(bytes,limits).map_err(Error::Header)?;
 let pixels=u64::from(dimensions.width)*u64::from(dimensions.height);
 // Conservative estimate, not OS-enforced allocation accounting.
 let estimate=pixels.checked_mul(16).and_then(|n|n.checked_add(bytes.len() as u64)).ok_or(Error::Limit)?;
 if estimate>max_work_bytes||pixels>80_000_000{return Err(Error::Limit);}
 let mut d=Decoder::new(Cursor::new(bytes));
 d.set_max_decoding_buffer_size(usize::try_from(max_work_bytes).unwrap_or(usize::MAX));
 d.read_info().map_err(Error::Decode)?;
 let info=d.info().ok_or(Error::Invalid)?;
 if u32::from(info.width)!=dimensions.width||u32::from(info.height)!=dimensions.height{return Err(Error::Invalid);}
 if !matches!(info.pixel_format,PixelFormat::RGB24|PixelFormat::L8){return Err(Error::Unsupported);}
 let data=d.decode().map_err(Error::Decode)?;
 let rgb=match info.pixel_format{
 PixelFormat::RGB24 if data.len() as u64==pixels*3=>data,
 PixelFormat::L8 if data.len() as u64==pixels=>{let mut rgb=Vec::with_capacity(pixels as usize*3);
 for v in data{rgb.extend([v;3]);}rgb},
 _=>return Err(Error::Invalid)};
 Ok(Rgb{dimensions,pixels:rgb})
}
#[cfg(test)]pub(crate) mod tests{
 use super::*;
 pub(crate) fn fixture()->Vec<u8>{
 let h="ffd8ffe000104a46494600010100000100010000ffdb0043000302020302020303030304030304050805050404050a070706080c0a0c0c0b0a0b0b0d0e12100d0e110e0b0b1016101113141515150c0f171816141812141514ffdb00430103040405040509050509140d0b0d1414141414141414141414141414141414141414141414141414141414141414141414141414141414141414141414141414ffc00011080002000203012200021101031101ffc4001500010100000000000000000000000000000006ffc40014100100000000000000000000000000000000ffc40014010100000000000000000000000000000006ffc40014110100000000000000000000000000000000ffda000c03010002110311003f009f008c7dffd9";
 (0..h.len()).step_by(2).map(|i|u8::from_str_radix(&h[i..i+2],16).unwrap()).collect()}
 fn limits()->Limits{Limits{max_header_bytes:1024,max_side:32,max_pixels:1024}}
 #[test]fn decodes_real_jpeg_fixture(){let r=decode(&fixture(),limits(),4096).unwrap();
 assert_eq!(r.dimensions,Dimensions{width:2,height:2});assert_eq!(r.pixels.len(),12);
 for p in r.pixels.chunks_exact(3){for (a,b) in p.iter().zip([100u8,120,140]){assert!(a.abs_diff(b)<=3);}}}
 #[test]fn rejects_budget_before_decode(){assert!(matches!(decode(&fixture(),limits(),10),Err(Error::Limit)));}
}
