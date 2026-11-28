//! Reference overlap radiometry and feather blending on aligned RGBA tiles.
//! Not a global seamline solver. The caller must supply spatially aligned windows.
#[derive(Debug,Clone,Copy,PartialEq,Eq)]
pub enum Error{Invalid,Limit,InsufficientOverlap}
const MAX_PIXELS:usize=16_000_000;
fn validate(a:&[u8],b:&[u8])->Result<usize,Error>{
 if a.len()!=b.len()||a.is_empty()||a.len()%4!=0{return Err(Error::Invalid);}
 let n=a.len()/4;if n>MAX_PIXELS{return Err(Error::Limit);}Ok(n)
}
/// Robust median additive correction in sRGB code values, excluding clipped pixels.
/// Limits prevent extreme corrections from low-information overlap.
pub fn median_offset(reference:&[u8],candidate:&[u8],min_samples:usize,max_abs:u8)->Result<[i16;3],Error>{
 let n=validate(reference,candidate)?;
 if min_samples==0||min_samples>n||max_abs>64{return Err(Error::Invalid);}
 let mut hist=[[0u32;511];3];let mut count=0usize;
 for (a,b) in reference.chunks_exact(4).zip(candidate.chunks_exact(4)){
  if a[3]!=255||b[3]!=255||a[..3].iter().chain(&b[..3]).any(|v|*v<8||*v>247){continue;}
  count+=1;for k in 0..3{hist[k][(i16::from(a[k])-i16::from(b[k])+255) as usize]+=1;}
 }
 if count<min_samples{return Err(Error::InsufficientOverlap);}
 let mut out=[0i16;3];
 for k in 0..3{let mut sum=0usize;for (i,&c) in hist[k].iter().enumerate(){sum+=c as usize;
  if sum>=(count+1)/2{out[k]=(i as i16-255).clamp(-i16::from(max_abs),i16::from(max_abs));break;}}}
 Ok(out)
}
/// Convert sRGB code values to linear light before feather blending.
fn linear(v:u8)->f64{let x=f64::from(v)/255.;if x<=0.04045{x/12.92}else{((x+0.055)/1.055).powf(2.4)}}
fn srgb(x:f64)->u8{let x=x.clamp(0.,1.);let v=if x<=0.0031308{12.92*x}else{1.055*x.powf(1./2.4)-0.055};(v*255.).round() as u8}
/// weights are candidate preference [0,1] in overlap; preserve valid-only pixels.
pub fn blend(a:&[u8],b:&[u8],weights:&[f32],offset:[i16;3],max_bytes:usize)->Result<Vec<u8>,Error>{
 let n=validate(a,b)?;
 if a.len()>max_bytes{return Err(Error::Limit);}
 if weights.len()!=n||offset.iter().any(|v|v.unsigned_abs()>64)||
    weights.iter().any(|v|!v.is_finite()||!(0.0..=1.0).contains(v))||
    a.chunks_exact(4).chain(b.chunks_exact(4)).any(|p|p[3]!=0&&p[3]!=255){return Err(Error::Invalid);}
 let mut out=vec![0;a.len()];
 for (i,w) in weights.iter().enumerate(){let j=i*4;let av=a[j+3]==255;let bv=b[j+3]==255;
  if !av&&!bv{continue;}let w=if !av{1.}else if !bv{0.}else{f64::from(*w)};
  for k in 0..3{let corrected=(i16::from(b[j+k])+offset[k]).clamp(0,255) as u8;
   out[j+k]=srgb((1.-w)*linear(a[j+k])+w*linear(corrected));}
  out[j+3]=255;
 }
 Ok(out)
}
#[cfg(test)]mod tests{
 use super::*;
 #[test]fn robust_offset(){assert_eq!(median_offset(&[100,100,100,255,120,120,120,255,250,250,250,255],&[90,90,90,255,110,110,110,255,10,10,10,255],2,32),Ok([10;3]));}
 #[test]fn linear_blend_and_nodata(){
 let a=[0,0,0,255,0,0,0,0];let b=[255,255,255,255,20,30,40,255];
 assert_eq!(blend(&a,&b,&[0.5,0.],[0;3],8).unwrap(),vec![188,188,188,255,20,30,40,255]);}
 #[test]fn rejects_bad_weights_and_limits(){assert_eq!(blend(&[0;4],&[0;4],&[f32::NAN],[0;3],4),Err(Error::Invalid));assert_eq!(blend(&[0;4],&[0;4],&[0.],[0;3],3),Err(Error::Limit));}
}
