//! Bounded COLMAP text import for undistorted pinhole cameras.
//! COLMAP world is NOT a projected geographic CRS. Explicit qualified alignment required.
use super::ortho::Camera;
use std::collections::{BTreeMap,BTreeSet};
#[derive(Debug,Clone,Copy,PartialEq,Eq)]
pub enum Error{Invalid,Unsupported,Limit,Duplicate,UnknownCamera}
#[derive(Debug,Clone,Copy)]
struct Intrinsic{width:u32,height:u32,fx:f64,fy:f64,cx:f64,cy:f64}
#[derive(Debug,Clone)]
pub struct View{pub id:u32,pub name:String,pub width:u32,pub height:u32,pub camera:Camera}
fn num<T:std::str::FromStr>(s:Option<&str>)->Result<T,Error>{s.ok_or(Error::Invalid)?.parse().map_err(|_|Error::Invalid)}
pub fn safe_name(s:&str)->bool{
 !s.is_empty()&&s.len()<=255&&!s.starts_with('.')&&s.bytes().all(|b|b.is_ascii_alphanumeric()||matches!(b,b'.'|b'_'|b'-'))
}
fn rot(q:[f64;4])->Result<[[f64;3];3],Error>{
 let n=q.iter().map(|v|v*v).sum::<f64>().sqrt();
 if !n.is_finite()||(n-1.).abs()>1e-5{return Err(Error::Invalid);}
 let [w,x,y,z]=q.map(|v|v/n);
 Ok([[1.-2.*(y*y+z*z),2.*(x*y-z*w),2.*(x*z+y*w)],
 [2.*(x*y+z*w),1.-2.*(x*x+z*z),2.*(y*z-x*w)],
 [2.*(x*z-y*w),2.*(y*z+x*w),1.-2.*(x*x+y*y)]])
}
pub fn parse(cameras:&str,images:&str,max_images:usize)->Result<Vec<View>,Error>{
 if max_images==0||max_images>100_000||cameras.len()>16*1024*1024||images.len()>128*1024*1024{return Err(Error::Limit);}
 let mut intr=BTreeMap::new();
 for line in cameras.lines().map(str::trim).filter(|s|!s.is_empty()&&!s.starts_with('#')){
 let mut t=line.split_whitespace();
 let id:u32=num(t.next())?;let model=t.next().ok_or(Error::Invalid)?;
 let width:u32=num(t.next())?;let height:u32=num(t.next())?;
 if width==0||height==0||width>65536||height>65536{return Err(Error::Invalid);}
 let(fx,fy,cx,cy)=match model{
 "PINHOLE"=>(num(t.next())?,num(t.next())?,num(t.next())?,num(t.next())?),
 "SIMPLE_PINHOLE"=>{let f=num(t.next())?;(f,f,num(t.next())?,num(t.next())?)},
 _=>return Err(Error::Unsupported)};
 let i=Intrinsic{width,height,fx,fy,cx,cy};
 if t.next().is_some()||![fx,fy,cx,cy].iter().all(|v|v.is_finite())||fx<=0.||fy<=0.{return Err(Error::Invalid);}
 if intr.insert(id,i).is_some(){return Err(Error::Duplicate);}
 if intr.len()>max_images{return Err(Error::Limit);}
 }
 let mut lines=images.lines();let mut result=Vec::new();let mut ids=BTreeSet::new();let mut names=BTreeSet::new();
 while let Some(line)=lines.next(){
 let line=line.trim();if line.is_empty()||line.starts_with('#'){continue;}
 if result.len()>=max_images{return Err(Error::Limit);}
 let mut t=line.split_whitespace();let id:u32=num(t.next())?;
 let q=[num(t.next())?,num(t.next())?,num(t.next())?,num(t.next())?];
 let trans:[f64;3]=[num(t.next())?,num(t.next())?,num(t.next())?];let cid:u32=num(t.next())?;
 let name=t.next().ok_or(Error::Invalid)?;
 if t.next().is_some()||!safe_name(name)||!trans.iter().all(|v|v.is_finite()){return Err(Error::Invalid);}
 if !ids.insert(id)||!names.insert(name){return Err(Error::Duplicate);}
 let i=intr.get(&cid).ok_or(Error::UnknownCamera)?;let r=rot(q)?;
 let center=std::array::from_fn(|j|-(0..3).map(|k|r[k][j]*trans[k]).sum::<f64>());
 let camera=Camera{world_to_camera:r,center,fx:i.fx,fy:i.fy,cx:i.cx,cy:i.cy};
 if !camera.valid(){return Err(Error::Invalid);}
 // COLMAP stores a second line of X Y POINT3D_ID triples (possibly empty).
 let mut observations=lines.next().ok_or(Error::Invalid)?.split_whitespace();
 while let Some(x)=observations.next(){
 let x:f64=num(Some(x))?;let y:f64=num(observations.next())?;let point:i64=num(observations.next())?;
 if !x.is_finite()||!y.is_finite()||point < -1{return Err(Error::Invalid);}
 }
 result.push(View{id,name:name.into(),width:i.width,height:i.height,camera});
 }
 if result.is_empty(){return Err(Error::Invalid);}
 Ok(result)
}
/// Explicit similarity from reconstruction coordinates to project metric coordinates.
/// Host must establish scale, rotation, offset and independent control accuracy.
#[derive(Debug,Clone,Copy)]
pub struct Alignment{pub scale:f64,pub rotation:[[f64;3];3],pub offset:[f64;3]}
pub fn align(view:&View,a:Alignment)->Result<Camera,Error>{
 if !a.scale.is_finite()||a.scale<=0.{return Err(Error::Invalid);}
 let check=Camera{world_to_camera:a.rotation,center:a.offset,fx:1.,fy:1.,cx:0.,cy:0.};
 if !check.valid(){return Err(Error::Invalid);}
 let c=view.camera;
 let center=std::array::from_fn(|i|a.offset[i]+a.scale*(0..3).map(|j|a.rotation[i][j]*c.center[j]).sum::<f64>());
 let r=std::array::from_fn(|i|std::array::from_fn(|j|(0..3).map(|k|c.world_to_camera[i][k]*a.rotation[j][k]).sum::<f64>()));
 let camera=Camera{center,world_to_camera:r,..c};
 if !camera.valid(){return Err(Error::Invalid);}Ok(camera)
}
#[cfg(test)]mod tests{
 use super::*;
 const C:&str="1 PINHOLE 100 80 50 51 49 39\n";
 #[test]fn imports_world_to_camera_and_alignment(){
 let v=parse(C,"1 1 0 0 0 1 2 3 1 image.jpg\n\n",10).unwrap();
 assert_eq!(v[0].camera.center,[-1.,-2.,-3.]);
 let c=align(&v[0],Alignment{scale:2.,rotation:[[0.,-1.,0.],[1.,0.,0.],[0.,0.,1.]],offset:[10.,20.,30.]}).unwrap();
 assert_eq!(c.center,[14.,18.,24.]);assert_eq!(c.world_to_camera,[[0.,1.,0.],[-1.,0.,0.],[0.,0.,1.]]);
 }
 #[test]fn rejects_distortion_and_path_traversal(){
 assert!(matches!(parse("1 SIMPLE_RADIAL 100 80 50 49 39 0.1","",10),Err(Error::Unsupported)));
 assert!(matches!(parse(C,"1 1 0 0 0 0 0 0 1 ../a.jpg\n\n",10),Err(Error::Invalid)));
 assert!(matches!(parse(C,"1 0 0 0 0 0 0 0 1 a.jpg\n\n",10),Err(Error::Invalid)));
 }
 #[test]fn empty_observation_line_and_duplicate_name(){
 assert_eq!(parse(C,"1 1 0 0 0 0 0 0 1 a.jpg\n\n2 1 0 0 0 0 0 0 1 b.jpg\n1 2 -1\n",10).unwrap().len(),2);
 assert!(matches!(parse(C,"1 1 0 0 0 0 0 0 1 a.jpg\n\n2 1 0 0 0 0 0 0 1 a.jpg\n\n",10),Err(Error::Duplicate)));
 }
}
