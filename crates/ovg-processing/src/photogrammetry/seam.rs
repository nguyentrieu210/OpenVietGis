//! Reference overlap radiometry and feather blending on aligned RGBA tiles.
//! Not a global seamline solver. The caller must supply spatially aligned windows.
#[derive(Debug,Clone,Copy,PartialEq,Eq)]
pub enum Error{Invalid,Limit,InsufficientOverlap,Cancelled}
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

/// Minimum-cost vertical seam through a rectangular aligned overlap.
/// Path moves at most one column per row. Both inputs must be valid on the
/// chosen path. This is a local two-image seam, not a global multi-label cut.
#[derive(Debug,Clone,Copy)]
pub struct SeamPolicy{pub max_pixels:usize,pub max_work_bytes:usize,pub step_penalty:u32,pub feather_px:u32}
pub fn vertical_seam(a:&[u8],b:&[u8],width:u32,height:u32,offset:[i16;3],p:SeamPolicy,
 mut cancelled:impl FnMut()->bool)->Result<Vec<u32>,Error>{
 let n=validate(a,b)?;
 let(w,h)=(width as usize,height as usize);
 if w==0||h==0||w.checked_mul(h)!=Some(n)||offset.iter().any(|v|v.unsigned_abs()>64)||
 p.step_penalty>10000||p.feather_px>1024{return Err(Error::Invalid);}
 let bytes=n.checked_add(w.checked_mul(16).ok_or(Error::Limit)?).and_then(|v|v.checked_add(h.checked_mul(4)?)).ok_or(Error::Limit)?;
 if n>p.max_pixels||bytes>p.max_work_bytes{return Err(Error::Limit);}
 const INF:u64=u64::MAX/4;
 let mut prev=vec![INF;w];let mut next=vec![INF;w];let mut parents=vec![0i8;n];
 for y in 0..h{
 if cancelled(){return Err(Error::Cancelled);}
 next.fill(INF);
 for x in 0..w{
 let i=(y*w+x)*4;
 if a[i+3]!=255||b[i+3]!=255{continue;}
 let mut local=0u64;
 for k in 0..3{
 let v=(i16::from(b[i+k])+offset[k]).clamp(0,255);
 local+=u64::from((i16::from(a[i+k])-v).unsigned_abs());
 }
 if y==0{next[x]=local;continue;}
 let mut best=INF;let mut parent=x;
 for j in x.saturating_sub(1)..=(x+1).min(w-1){
 if prev[j]==INF{continue;}
 let candidate=prev[j]+if j==x{0}else{u64::from(p.step_penalty)};
 if candidate<best||(candidate==best&&j==x){best=candidate;parent=j;}
 }
 if best!=INF{next[x]=best+local;parents[y*w+x]=(parent as isize-x as isize) as i8;}
 }
 std::mem::swap(&mut prev,&mut next);
 }
 let mut x=(0..w).min_by_key(|&i|(prev[i],i)).ok_or(Error::Invalid)?;
 if prev[x]==INF{return Err(Error::InsufficientOverlap);}
 let mut path=vec![0u32;h];
 for y in (0..h).rev(){path[y]=x as u32;if y>0{x=(x as isize+isize::from(parents[y*w+x])) as usize;}}
 Ok(path)
}
/// Blend around a validated vertical seam; left side chooses A, right side B.
pub fn blend_seam(a:&[u8],b:&[u8],width:u32,height:u32,path:&[u32],offset:[i16;3],p:SeamPolicy)->Result<Vec<u8>,Error>{
 let n=validate(a,b)?;
 let(w,h)=(width as usize,height as usize);
 if w==0||h==0||w.checked_mul(h)!=Some(n)||path.len()!=h||
 path.iter().any(|&x|x>=width)||path.windows(2).any(|v|v[0].abs_diff(v[1])>1)||p.feather_px>1024{return Err(Error::Invalid);}
 if n>p.max_pixels||n.checked_mul(8).ok_or(Error::Limit)?>p.max_work_bytes{return Err(Error::Limit);}
 let weights:Vec<f32>=(0..n).map(|i|{
 let d=(i%w) as f64-f64::from(path[i/w]);
 if p.feather_px==0{if d<0.{0.}else{1.}}else{(0.5+d/(2.*f64::from(p.feather_px))).clamp(0.,1.) as f32}
 }).collect();
 blend(a,b,&weights,offset,n*4)
}
#[cfg(test)]mod tests{
 use super::*;
 #[test]fn robust_offset(){assert_eq!(median_offset(&[100,100,100,255,120,120,120,255,250,250,250,255],&[90,90,90,255,110,110,110,255,10,10,10,255],2,32),Ok([10;3]));}
 #[test]fn linear_blend_and_nodata(){
 let a=[0,0,0,255,0,0,0,0];let b=[255,255,255,255,20,30,40,255];
 assert_eq!(blend(&a,&b,&[0.5,0.],[0;3],8).unwrap(),vec![188,188,188,255,20,30,40,255]);}
 #[test]fn chooses_low_difference_corridor_and_blends(){
 let a=vec![100u8,100,100,255].repeat(12);let mut b=vec![200u8,200,200,255].repeat(12);
 for y in 0..3{b[(y*4+2)*4..(y*4+2)*4+3].fill(100);}
 let p=SeamPolicy{max_pixels:100,max_work_bytes:4096,step_penalty:10,feather_px:0};
 let path=vertical_seam(&a,&b,4,3,[0;3],p,||false).unwrap();assert_eq!(path,vec![2;3]);
 let result=blend_seam(&a,&b,4,3,&path,[0;3],p).unwrap();assert_eq!(result[0],100);assert_eq!(result[12],200);
 for x in 0..4{b[(4+x)*4+3]=0;}
 assert_eq!(vertical_seam(&a,&b,4,3,[0;3],p,||false),Err(Error::InsufficientOverlap));
 assert_eq!(vertical_seam(&a,&b,4,3,[0;3],p,||true),Err(Error::Cancelled));
 }
 #[test]fn rejects_bad_weights_and_limits(){assert_eq!(blend(&[0;4],&[0;4],&[f32::NAN],[0;3],4),Err(Error::Invalid));assert_eq!(blend(&[0;4],&[0;4],&[0.],[0;3],3),Err(Error::Limit));}
}
