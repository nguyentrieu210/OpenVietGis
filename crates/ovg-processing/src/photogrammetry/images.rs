//! Bounded header inspection and image pyramid planning. No pixel decoder yet.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Dimensions { pub width:u32, pub height:u32 }
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Error { Invalid, Truncated, Unsupported, Limit, Overflow }
#[derive(Debug, Clone, Copy)]
pub struct Limits { pub max_header_bytes:usize, pub max_side:u32, pub max_pixels:u64 }
impl Limits {
    pub fn check(self,d:Dimensions)->Result<Dimensions,Error>{
        if d.width==0||d.height==0{return Err(Error::Invalid);}
        if d.width>self.max_side||d.height>self.max_side||
            u64::from(d.width)*u64::from(d.height)>self.max_pixels{return Err(Error::Limit);}
        Ok(d)
    }
}
/// Inspect JPEG SOF in a bounded prefix, without trusting EXIF or decompressing pixels.
pub fn jpeg_dimensions(bytes:&[u8],limits:Limits)->Result<Dimensions,Error>{
    let b=&bytes[..bytes.len().min(limits.max_header_bytes)];
    if b.get(..2)!=Some(&[0xff,0xd8][..]){return Err(Error::Invalid);}
    let mut pos=2usize;
    while pos<b.len(){
        if b[pos]!=0xff{return Err(Error::Invalid);}
        while b.get(pos)==Some(&0xff){pos+=1;}
        let marker=*b.get(pos).ok_or(Error::Truncated)?;pos+=1;
        if marker==0xda||marker==0xd9{return Err(Error::Unsupported);}
        if marker==0x00||marker==0xd8||(0xd0..=0xd7).contains(&marker){return Err(Error::Invalid);}
        if marker==0x01{continue;}
        let len=b.get(pos..pos+2).ok_or(Error::Truncated)?;
        let n=u16::from_be_bytes([len[0],len[1]]) as usize;
        if n<2{return Err(Error::Invalid);}
        let end=pos.checked_add(n).ok_or(Error::Overflow)?;
        let seg=b.get(pos..end).ok_or(Error::Truncated)?;
        if matches!(marker,0xc0|0xc1|0xc2|0xc3|0xc5|0xc6|0xc7|0xc9|0xca|0xcb|0xcd|0xce|0xcf){
            if seg.len()<8{return Err(Error::Invalid);}
            let comps=seg[7] as usize;
            if comps==0||seg.len()!=8+3*comps{return Err(Error::Invalid);}
            return limits.check(Dimensions{height:u16::from_be_bytes([seg[3],seg[4]]) as u32,
                width:u16::from_be_bytes([seg[5],seg[6]]) as u32});
        }
        pos=end;
    }
    Err(Error::Truncated)
}
/// Full-resolution level first, ceil division for odd dimensions.
pub fn pyramid(d:Dimensions,stop_side:u32,max_levels:u8)->Result<Vec<Dimensions>,Error>{
    if d.width==0||d.height==0||stop_side==0||max_levels==0||max_levels>33{return Err(Error::Invalid);}
    let mut out=vec![d];let mut current=d;
    while current.width>stop_side||current.height>stop_side{
        if out.len()>=usize::from(max_levels){return Err(Error::Limit);}
        current=Dimensions{width:current.width.div_ceil(2),height:current.height.div_ceil(2)};
        out.push(current);
    }
    Ok(out)
}
pub fn working_bytes(d:Dimensions,channels:u8,bytes_per_channel:u8,buffers:u16)->Result<u64,Error>{
    if d.width==0||d.height==0||channels==0||bytes_per_channel==0||buffers==0{return Err(Error::Invalid);}
    u64::from(d.width).checked_mul(u64::from(d.height))
        .and_then(|n|n.checked_mul(u64::from(channels)))
        .and_then(|n|n.checked_mul(u64::from(bytes_per_channel)))
        .and_then(|n|n.checked_mul(u64::from(buffers))).ok_or(Error::Overflow)
}
#[cfg(test)]
mod tests{
    use super::*;
    fn jpeg()->Vec<u8>{vec![0xff,0xd8,0xff,0xe0,0,4,1,2,0xff,0xc0,0,11,8,0,7,0,9,1,1,0x11,0]}
    fn limits()->Limits{Limits{max_header_bytes:128,max_side:32,max_pixels:1024}}
    #[test] fn reads_bounded_sof(){assert_eq!(jpeg_dimensions(&jpeg(),limits()),Ok(Dimensions{width:9,height:7}));}
    #[test] fn rejects_truncation_and_large_image(){let mut l=limits();l.max_header_bytes=12;
        assert_eq!(jpeg_dimensions(&jpeg(),l),Err(Error::Truncated));l=limits();l.max_pixels=40;
        assert_eq!(jpeg_dimensions(&jpeg(),l),Err(Error::Limit));}
    #[test] fn odd_pyramid_and_level_limit(){let d=Dimensions{width:9,height:7};
        assert_eq!(pyramid(d,2,5).unwrap(),vec![d,Dimensions{width:5,height:4},Dimensions{width:3,height:2},Dimensions{width:2,height:1}]);
        assert_eq!(pyramid(d,2,2),Err(Error::Limit));}
    #[test] fn memory_overflow_detected(){assert_eq!(working_bytes(Dimensions{width:u32::MAX,height:u32::MAX},4,4,8),Err(Error::Overflow));}
}
