//! Reference terrain-grid mesh with local coordinates and north-up raster UVs.
//! Skips nodata triangles; no arbitrary-surface reconstruction or texture atlas.
use super::ortho::Grid;
#[derive(Debug,Clone,Copy,PartialEq)]
pub struct Vertex{pub xyz:[f32;3],pub uv:[f32;2]}
#[derive(Debug,Clone,PartialEq)]
pub struct Mesh{pub origin:[f64;3],pub vertices:Vec<Vertex>,pub triangles:Vec<[u32;3]>}
#[derive(Debug,Clone,Copy,PartialEq,Eq)]
pub enum Error{Invalid,Limit}
/// Local origin avoids f32 precision loss for large projected coordinates.
pub fn terrain(grid:Grid,z:&[f32],z_origin:f64,max_vertices:usize,max_triangles:usize,max_edge_z:f32)->Result<Mesh,Error>{
 if grid.width<2||grid.height<2||!grid.gsd.is_finite()||grid.gsd<=0.||!grid.origin_x.is_finite()||
 !grid.origin_y.is_finite()||!z_origin.is_finite()||!max_edge_z.is_finite()||max_edge_z<=0.{return Err(Error::Invalid);}
 let n=u64::from(grid.width)*u64::from(grid.height);
 if n>4_000_000||n>max_vertices as u64{return Err(Error::Limit);}
 if z.len() as u64!=n{return Err(Error::Invalid);}
 let mut vertices=Vec::new();let mut ids=vec![None;n as usize];
 for (i,h) in z.iter().enumerate(){
  if !h.is_finite(){continue;}
  let x=i%grid.width as usize;let y=i/grid.width as usize;
  let xyz=[(x as f64+0.5)*grid.gsd,-(y as f64+0.5)*grid.gsd,f64::from(*h)-z_origin];
  if xyz.iter().any(|v|!v.is_finite()||v.abs()>f32::MAX as f64){return Err(Error::Invalid);}
  ids[i]=Some(vertices.len() as u32);
  vertices.push(Vertex{xyz:xyz.map(|v|v as f32),uv:[(x as f32+0.5)/grid.width as f32,(y as f32+0.5)/grid.height as f32]});
 }
 let mut triangles=Vec::new();let w=grid.width as usize;
 for y in 0..grid.height as usize-1{for x in 0..w-1{
  let a=y*w+x;let b=a+1;let c=a+w;let d=c+1;
  for t in [[a,c,b],[b,c,d]]{
   let [Some(i),Some(j),Some(k)]=t.map(|v|ids[v]) else{continue;};
   let heights=t.map(|v|z[v]);let min=heights.iter().copied().fold(f32::INFINITY,f32::min);
   let max=heights.iter().copied().fold(f32::NEG_INFINITY,f32::max);
   if max-min>max_edge_z{continue;}
   if triangles.len()>=max_triangles||triangles.len()>=8_000_000{return Err(Error::Limit);}
   triangles.push([i,j,k]);
  }
 }}
 Ok(Mesh{origin:[grid.origin_x,grid.origin_y,z_origin],vertices,triangles})
}
#[cfg(test)]mod tests{
 use super::*;
 fn grid()->Grid{Grid{origin_x:500000.,origin_y:1200000.,gsd:1.,width:2,height:2}}
 #[test]fn local_coordinates_uv_and_upward_winding(){
 let m=terrain(grid(),&[10.;4],10.,4,2,2.).unwrap();
 assert_eq!(m.vertices[0],Vertex{xyz:[0.5,-0.5,0.],uv:[0.25,0.25]});
 assert_eq!(m.triangles,vec![[0,2,1],[1,2,3]]);
 }
 #[test]fn holes_and_discontinuity(){let m=terrain(grid(),&[10.,10.,10.,f32::NAN],10.,4,2,2.).unwrap();assert_eq!(m.triangles.len(),1);
 let m=terrain(grid(),&[10.,10.,10.,100.],10.,4,2,2.).unwrap();assert_eq!(m.triangles.len(),1);}
 #[test]fn respects_triangle_limit(){assert_eq!(terrain(grid(),&[10.;4],10.,4,1,2.),Err(Error::Limit));}
}
