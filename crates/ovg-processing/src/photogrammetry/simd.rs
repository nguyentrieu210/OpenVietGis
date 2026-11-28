//! Exact opaque-pixel counts. Runtime-dispatched AVX2/NEON with scalar tails.
//! Integer operations preserve the serial coverage count, including alpha=254.
fn scalar(bytes:&[u8])->u64{bytes.chunks_exact(4).filter(|p|p[3]==255).count() as u64}
pub fn opaque_rgba(bytes:&[u8])->u64{
 #[cfg(all(any(target_arch="x86",target_arch="x86_64"),target_endian="little"))]
 if std::arch::is_x86_feature_detected!("avx2"){
  // SAFETY: runtime feature detection; implementation only loads full 32-byte chunks.
  return unsafe{avx2(bytes)};
 }
 #[cfg(all(target_arch="aarch64",target_endian="little"))]
 if std::arch::is_aarch64_feature_detected!("neon"){
  // SAFETY: runtime feature detection; implementation only loads full 16-byte chunks.
  return unsafe{neon(bytes)};
 }
 scalar(bytes)
}
#[cfg(all(any(target_arch="x86",target_arch="x86_64"),target_endian="little"))]
#[target_feature(enable="avx2")]
unsafe fn avx2(bytes:&[u8])->u64{
 #[cfg(target_arch="x86")]use std::arch::x86::*;
 #[cfg(target_arch="x86_64")]use std::arch::x86_64::*;
 let mut n=0u64;let mut chunks=bytes.chunks_exact(32);
 for c in &mut chunks{
  // SAFETY: unaligned load of exactly 32 readable bytes.
  let v=unsafe{_mm256_loadu_si256(c.as_ptr().cast())};
  let a=_mm256_srli_epi32::<24>(v);
  let eq=_mm256_cmpeq_epi32(a,_mm256_set1_epi32(255));
  n+=u64::from((_mm256_movemask_ps(_mm256_castsi256_ps(eq)) as u32).count_ones());
 }
 n+scalar(chunks.remainder())
}
#[cfg(all(target_arch="aarch64",target_endian="little"))]
#[target_feature(enable="neon")]
unsafe fn neon(bytes:&[u8])->u64{
 use std::arch::aarch64::*;
 let mut n=0u64;let mut chunks=bytes.chunks_exact(16);
 for c in &mut chunks{
  // SAFETY: unaligned byte load of exactly 16 readable bytes.
  let v=unsafe{vld1q_u8(c.as_ptr())};
  let a=vshrq_n_u32::<24>(vreinterpretq_u32_u8(v));
  let eq=vceqq_u32(a,vdupq_n_u32(255));
  n+=u64::from(vaddvq_u32(vshrq_n_u32::<31>(eq)));
 }
 n+scalar(chunks.remainder())
}
#[cfg(test)]mod tests{
 use super::*;
 #[test]fn matches_scalar_for_every_tail_and_misalignment(){
 let mut b=vec![0u8;8193];for(i,v)in b.iter_mut().enumerate(){*v=if i%7==0{255}else if i%3==0{254}else{(i%251)as u8};}
 for offset in 0..32{for tail in 0..40{let s=&b[offset..b.len()-tail];assert_eq!(opaque_rgba(s),scalar(s));}}
 }
 #[test]fn ignores_nonopaque_and_incomplete_pixels(){
 assert_eq!(opaque_rgba(&[255,255,255,254,0,0,0,255,255]),1);
 assert_eq!(opaque_rgba(&[]),0);
 }
}
