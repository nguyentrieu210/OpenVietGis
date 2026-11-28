//! Reference calibrated reflectance and NDVI kernels. No sensor file parser or model execution.
//! Band co-registration and calibration provenance must be independently verified by host.
#[derive(Debug,Clone,Copy,PartialEq,Eq)]
pub enum Band{Red,NearInfrared,Green,RedEdge,Blue}
#[derive(Debug,Clone,Copy)]
pub struct Calibration{pub gain:f64,pub offset:f64,pub nodata:Option<u16>,pub saturation:u16}
pub struct RawBand<'a>{pub band:Band,pub samples:&'a[u16],pub calibration:Calibration}
#[derive(Debug,Clone,Copy,PartialEq,Eq)]
pub enum Error{Invalid,Limit,WrongBand,Unregistered}
fn valid(c:Calibration)->bool{c.gain.is_finite()&&c.gain>0.&&c.offset.is_finite()&&c.saturation>0}
fn reflectance(sample:u16,c:Calibration)->Option<f64>{
 if c.nodata==Some(sample)||sample>=c.saturation{return None;}
 let r=f64::from(sample)*c.gain+c.offset;
 if !r.is_finite()||!(0.0..=1.0).contains(&r){None}else{Some(r)}
}
/// The grid IDs must refer to host-verified identical CRS, affine grid and extent.
/// NDVI uses calibrated NIR/red, never substitutes visible RGB.
pub fn ndvi(red:RawBand<'_>,nir:RawBand<'_>,red_grid:&str,nir_grid:&str,max_cells:usize)->Result<Vec<f32>,Error>{
 if red.band!=Band::Red||nir.band!=Band::NearInfrared{return Err(Error::WrongBand);}
 if !super::valid_id(red_grid)||red_grid!=nir_grid{return Err(Error::Unregistered);}
 let n=red.samples.len();
 if n>max_cells||n>16_000_000{return Err(Error::Limit);}
 if n==0||nir.samples.len()!=n||!valid(red.calibration)||!valid(nir.calibration){return Err(Error::Invalid);}
 let mut out=Vec::with_capacity(n);
 for (&r,&i) in red.samples.iter().zip(nir.samples){
 let v=match(reflectance(r,red.calibration),reflectance(i,nir.calibration)){
 (Some(r),Some(i))if r+i>1e-9=>((i-r)/(i+r)) as f32,_=>f32::NAN};
 out.push(v);
 }
 Ok(out)
}
#[cfg(test)]mod tests{
 use super::*;
 fn b(band:Band,samples:&[u16])->RawBand<'_>{RawBand{band,samples,calibration:Calibration{gain:0.001,offset:0.,nodata:Some(65535),saturation:1001}}}
 #[test]fn known_reflectance_and_nodata(){
 let v=ndvi(b(Band::Red,&[200,0,65535,1002]),b(Band::NearInfrared,&[600,0,600,600]),"g","g",4).unwrap();
 assert!((v[0]-0.5).abs()<1e-6);assert!(v[1..].iter().all(|v|v.is_nan()));}
 #[test]fn rejects_rgb_and_grid_mismatch(){
 assert_eq!(ndvi(b(Band::Red,&[200]),b(Band::Green,&[600]),"g","g",1),Err(Error::WrongBand));
 assert_eq!(ndvi(b(Band::Red,&[200]),b(Band::NearInfrared,&[600]),"g","other",1),Err(Error::Unregistered));}
}
