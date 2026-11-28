//! Reference depth-map backprojection in calibrated camera coordinates.
//! Input depth is positive camera Z, not ray range. No stereo estimation here.
use super::ortho::Camera;
#[derive(Debug,Clone,Copy,PartialEq)]
pub struct Point {pub xyz:[f64;3],pub rgb:Option<[u8;3]>,pub confidence:f32}
#[derive(Debug,Clone,Copy)]
pub struct Policy {pub min_depth:f32,pub max_depth:f32,pub min_confidence:f32,pub stride:u32,pub max_points:usize}
#[derive(Debug,Clone,Copy,PartialEq,Eq)]
pub enum Error{Invalid,Limit}
pub struct DepthMap<'a>{pub width:u32,pub height:u32,pub z:&'a[f32],pub confidence:Option<&'a[f32]>,pub rgb:Option<&'a[u8]>}
pub fn backproject(map:DepthMap<'_>,camera:Camera,p:Policy)->Result<Vec<Point>,Error>{
    if !camera.valid()||map.width==0||map.height==0||p.stride==0||p.max_points==0||p.max_points>5_000_000||
        !p.min_depth.is_finite()||!p.max_depth.is_finite()||p.min_depth<=0.||p.max_depth<p.min_depth||
        !p.min_confidence.is_finite()||!(0.0..=1.0).contains(&p.min_confidence){return Err(Error::Invalid);}
    let n=u64::from(map.width)*u64::from(map.height);
    if n>100_000_000{return Err(Error::Limit);}
    if map.z.len() as u64!=n||map.confidence.is_some_and(|c|c.len() as u64!=n)||
        map.rgb.is_some_and(|c|Some(c.len() as u64)!=n.checked_mul(3)){return Err(Error::Invalid);}
    let count=u64::from(map.width.div_ceil(p.stride))*u64::from(map.height.div_ceil(p.stride));
    if count>p.max_points as u64{return Err(Error::Limit);}
    let mut out=Vec::with_capacity(count as usize);
    for y in (0..map.height).step_by(p.stride as usize){for x in (0..map.width).step_by(p.stride as usize){
        let i=y as usize*map.width as usize+x as usize;let z=map.z[i];
        let c=map.confidence.map_or(1.,|v|v[i]);
        if !z.is_finite()||z<p.min_depth||z>p.max_depth||!c.is_finite()||!(0.0..=1.0).contains(&c)||c<p.min_confidence{continue;}
        let z=f64::from(z);let v=[(f64::from(x)-camera.cx)*z/camera.fx,(f64::from(y)-camera.cy)*z/camera.fy,z];
        let mut xyz=camera.center;
        for (i,coord) in xyz.iter_mut().enumerate(){for (j,val) in v.iter().enumerate(){*coord+=camera.world_to_camera[j][i]*val;}}
        if !xyz.iter().all(|x|x.is_finite()){return Err(Error::Invalid);}
        let rgb=map.rgb.map(|b|[b[i*3],b[i*3+1],b[i*3+2]]);
        out.push(Point{xyz,rgb,confidence:c});
    }}
    Ok(out)
}
#[cfg(test)]mod tests{
 use super::*;
 fn cam()->Camera{Camera{world_to_camera:[[1.,0.,0.],[0.,-1.,0.],[0.,0.,-1.]],center:[0.,0.,10.],fx:2.,fy:2.,cx:0.,cy:0.}}
 fn p()->Policy{Policy{min_depth:1.,max_depth:20.,min_confidence:0.5,stride:1,max_points:10}}
 #[test]fn camera_z_backprojects_to_world(){
 let pts=backproject(DepthMap{width:2,height:1,z:&[10.,10.],confidence:None,rgb:Some(&[1,2,3,4,5,6])},cam(),p()).unwrap();
 assert_eq!(pts[1].xyz,[5.,0.,0.]);assert_eq!(pts[1].rgb,Some([4,5,6]));}
 #[test]fn nodata_confidence_and_cap(){
 let m=DepthMap{width:3,height:1,z:&[10.,f32::NAN,10.],confidence:Some(&[1.,1.,0.2]),rgb:None};
 assert_eq!(backproject(m,cam(),p()).unwrap().len(),1);
 let mut q=p();q.max_points=1;
 assert_eq!(backproject(DepthMap{width:2,height:1,z:&[10.,10.],confidence:None,rgb:None},cam(),q),Err(Error::Limit));}
}
