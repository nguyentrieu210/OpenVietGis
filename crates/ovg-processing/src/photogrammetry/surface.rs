//! Bounded point-to-raster aggregation. DTM uses externally classified ground only.
//! No interpolation, ground classification, CRS conversion or survey certification.
use super::ortho::Grid;
#[derive(Debug,Clone,Copy,PartialEq,Eq)]
pub enum Class{Ground,NonGround,Unknown}
#[derive(Debug,Clone,Copy)]
pub struct Sample{pub xyz:[f64;3],pub class:Class}
#[derive(Debug,Clone,Copy,PartialEq,Eq)]
pub enum Product{Dsm,Dtm}
#[derive(Debug,Clone,Copy,PartialEq,Eq)]
pub enum Error{Invalid,Limit}
#[derive(Debug)]
pub struct Raster{pub elevations:Vec<f32>,pub count:Vec<u32>}
pub fn rasterize(grid:Grid,points:impl IntoIterator<Item=Sample>,product:Product,max_cells:usize,max_points:u64,min_points:u32)->Result<Raster,Error>{
 if grid.width==0||grid.height==0||!grid.origin_x.is_finite()||!grid.origin_y.is_finite()||
    !grid.gsd.is_finite()||grid.gsd<=0.||min_points==0||max_points==0{return Err(Error::Invalid);}
 let n=u64::from(grid.width)*u64::from(grid.height);
 if n>max_cells as u64||n>16_000_000{return Err(Error::Limit);}
 let n=n as usize;let mut elevations=vec![f32::NAN;n];let mut count=vec![0u32;n];
 for (i,p) in points.into_iter().enumerate(){
    if i as u64>=max_points{return Err(Error::Limit);}
    if !p.xyz.iter().all(|v|v.is_finite())||p.xyz[2].abs()>f32::MAX as f64{return Err(Error::Invalid);}
    if product==Product::Dtm&&p.class!=Class::Ground{continue;}
    let x=((p.xyz[0]-grid.origin_x)/grid.gsd).floor();let y=((grid.origin_y-p.xyz[1])/grid.gsd).floor();
    if x<0.||y<0.||x>=f64::from(grid.width)||y>=f64::from(grid.height){continue;}
    let idx=y as usize*grid.width as usize+x as usize;let z=p.xyz[2] as f32;
    count[idx]=count[idx].checked_add(1).ok_or(Error::Limit)?;
    if elevations[idx].is_nan()||match product{Product::Dsm=>z>elevations[idx],Product::Dtm=>z<elevations[idx]}{elevations[idx]=z;}
 }
 for (z,c) in elevations.iter_mut().zip(&count){if *c<min_points{*z=f32::NAN;}}
 Ok(Raster{elevations,count})
}
#[cfg(test)]mod tests{
 use super::*;
 fn grid()->Grid{Grid{origin_x:0.,origin_y:2.,gsd:1.,width:2,height:2}}
 fn p(z:f64,class:Class)->Sample{Sample{xyz:[0.5,1.5,z],class}}
 #[test]fn dsm_and_classified_ground_dtm(){
 let pts=[p(5.,Class::Ground),p(20.,Class::NonGround),p(1.,Class::Unknown)];
 let d=rasterize(grid(),pts,Product::Dsm,4,10,1).unwrap();assert_eq!(d.elevations[0],20.);assert_eq!(d.count[0],3);
 let d=rasterize(grid(),pts,Product::Dtm,4,10,1).unwrap();assert_eq!(d.elevations[0],5.);assert!(d.elevations[1].is_nan());}
 #[test]fn no_ground_no_dtm_and_limit(){
 let d=rasterize(grid(),[p(20.,Class::Unknown)],Product::Dtm,4,10,1).unwrap();assert!(d.elevations.iter().all(|v|v.is_nan()));
 assert!(matches!(rasterize(grid(),[],Product::Dsm,3,10,1),Err(Error::Limit)));}
}
