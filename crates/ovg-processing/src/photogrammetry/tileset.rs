//! Reference 3D Tiles 1.1 tileset for independently georeferenced GLB leaves.
//! Each leaf has a host-verified rigid local Z-up metre frame in ECEF.
//! No projected-CRS transform, nonlinear reprojection, mesh simplification or LOD generation.
use serde_json::{json,Value};
#[derive(Debug,Clone,Copy)]
pub struct Bounds{pub min:[f64;3],pub max:[f64;3]}
#[derive(Debug,Clone)]
pub struct Leaf{
 pub sha256:String,pub bounds:Bounds,pub origin_ecef:[f64;3],
 /// Three unit basis vectors (local east, north, up), as columns.
 pub axes:[[f64;3];3],
}
#[derive(Debug,Clone,Copy,PartialEq,Eq)]
pub enum Error{Invalid,Limit,Duplicate}
fn dot(a:[f64;3],b:[f64;3])->f64{(0..3).map(|i|a[i]*b[i]).sum()}
fn cross(a:[f64;3],b:[f64;3])->[f64;3]{[a[1]*b[2]-a[2]*b[1],a[2]*b[0]-a[0]*b[2],a[0]*b[1]-a[1]*b[0]]}
impl Bounds{
 fn valid(self)->bool{(0..3).all(|i|self.min[i].is_finite()&&self.max[i].is_finite()&&self.min[i]<=self.max[i]&&(self.max[i]-self.min[i]).is_finite())}
 fn box_array(self)->[f64;12]{
 let c:[f64;3]=std::array::from_fn(|i|self.min[i]+(self.max[i]-self.min[i])*0.5);
 let h:[f64;3]=std::array::from_fn(|i|((self.max[i]-self.min[i])*0.5).max(1e-6));
 [c[0],c[1],c[2],h[0],0.,0.,0.,h[1],0.,0.,0.,h[2]]
 }
}
impl Leaf{
 fn valid(&self)->bool{
 super::digest(&self.sha256)&&self.bounds.valid()&&self.origin_ecef.iter().all(|x|x.is_finite())&&
 self.origin_ecef[0].hypot(self.origin_ecef[1]).hypot(self.origin_ecef[2])>=6_000_000.&&
 self.origin_ecef[0].hypot(self.origin_ecef[1]).hypot(self.origin_ecef[2])<=7_000_000.&&
 self.axes.iter().flatten().all(|x|x.is_finite())&&
 (0..3).all(|i|(0..3).all(|j|(dot(self.axes[i],self.axes[j])-if i==j{1.}else{0.}).abs()<1e-8))&&
 (dot(cross(self.axes[0],self.axes[1]),self.axes[2])-1.).abs()<1e-8&&
 (0..3).all(|i|self.bounds.min[i].abs()<=100_000.&&self.bounds.max[i].abs()<=100_000.)
 }
 fn world(&self,v:[f64;3])->[f64;3]{
 std::array::from_fn(|i|self.origin_ecef[i]+(0..3).map(|j|self.axes[j][i]*v[j]).sum::<f64>())
 }
 fn transform(&self)->[f64;16]{
 let [x,y,z]=self.axes;let t=self.origin_ecef;
 [x[0],x[1],x[2],0.,y[0],y[1],y[2],0.,z[0],z[1],z[2],0.,t[0],t[1],t[2],1.]
 }
}
/// Content URI is a deterministic relative digest filename, never arbitrary input.
/// Root has no content; leaves are independent, not a multiresolution pyramid.
pub fn build(leaves:&[Leaf],max_json_bytes:usize)->Result<Vec<u8>,Error>{
 if leaves.is_empty()||leaves.len()>100_000||max_json_bytes==0||max_json_bytes>128*1024*1024{return Err(Error::Limit);}
 let mut root=Bounds{min:[f64::INFINITY;3],max:[f64::NEG_INFINITY;3]};
 let mut seen=std::collections::BTreeSet::new();let mut children=Vec::<Value>::new();
 for l in leaves{
 if !l.valid(){return Err(Error::Invalid);}
 if !seen.insert(&l.sha256){return Err(Error::Duplicate);}
 // Root world bounds must enclose transformed leaf boxes, including degenerate-axis padding.
 let b=l.bounds.box_array();
 for k in 0..8{
 let p=std::array::from_fn(|i|b[i]+if k&(1<<i)==0{-b[3+i*4]}else{b[3+i*4]});
 let p=l.world(p);
 for i in 0..3{root.min[i]=root.min[i].min(p[i]);root.max[i]=root.max[i].max(p[i]);}
 }
 children.push(json!({"boundingVolume":{"box":l.bounds.box_array()},"geometricError":0,
 "transform":l.transform(),"content":{"uri":format!("content/{}.glb",l.sha256)}}));
 }
 if !root.valid(){return Err(Error::Invalid);}
 let error=(root.max[0]-root.min[0]).hypot(root.max[1]-root.min[1]).hypot(root.max[2]-root.min[2]).max(1.);
 let doc=json!({"asset":{"version":"1.1"},"geometricError":error,
 "root":{"boundingVolume":{"box":root.box_array()},"geometricError":error,"refine":"ADD","children":children}});
 let result=serde_json::to_vec(&doc).map_err(|_|Error::Invalid)?;
 if result.len()>max_json_bytes{return Err(Error::Limit);}Ok(result)
}
#[cfg(test)]mod tests{
 use super::*;
 fn leaf()->Leaf{Leaf{sha256:"a".repeat(64),bounds:Bounds{min:[0.,0.,0.],max:[2.,2.,1.]},
 origin_ecef:[6378137.,0.,0.],axes:[[0.,1.,0.],[0.,0.,1.],[1.,0.,0.]]}}
 #[test]fn ecef_transform_and_safe_uri(){
 let b=build(&[leaf()],4096).unwrap();let j:Value=serde_json::from_slice(&b).unwrap();
 assert_eq!(j["asset"]["version"],"1.1");assert_eq!(j["root"]["children"][0]["transform"][12],6378137.);
 assert_eq!(j["root"]["boundingVolume"]["box"][0],6378137.5);
 assert_eq!(j["root"]["children"][0]["content"]["uri"],format!("content/{}.glb","a".repeat(64)));
 }
 #[test]fn rejects_reflection_and_duplicate(){
 let mut l=leaf();l.axes[2]=[-1.,0.,0.];assert_eq!(build(&[l],4096),Err(Error::Invalid));
 assert_eq!(build(&[leaf(),leaf()],4096),Err(Error::Duplicate));}
}
