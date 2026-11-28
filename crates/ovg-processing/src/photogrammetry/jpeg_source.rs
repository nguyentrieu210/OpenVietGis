//! Bounded JPEG source for already-undistorted COLMAP imagery.
//! Host must verify the undistortion output, camera model, explicit alignment,
//! DEM CRS and input digests. Never use distorted raw JPEGs as pinhole imagery.
use super::{colmap_model,jpeg,images,ortho,pipeline,tiles,image_cache};
use std::sync::Arc;
use std::{collections::BTreeMap,fs::{self,File},io::Read,path::{Path,PathBuf}};
#[derive(Debug,Clone,Copy)]
pub struct Policy{pub max_compressed_bytes:u64,pub max_decode_bytes:u64,pub cache_bytes:u64,pub header:images::Limits}
#[derive(Debug,Clone)]
struct View{path:PathBuf,camera:ortho::Camera,width:u32,height:u32,footprint:Option<tiles::Window>}
pub struct Source{dem:pipeline::RawSource,views:Vec<View>,policy:Policy,cache:image_cache::Cache}
impl Source{
 /// Footprints are conservative bounds in output pixel grid, not GPS alone.
 /// Missing footprint means the image remains a candidate for every tile.
 pub fn from_model(root:&Path,dem_path:PathBuf,width:u32,height:u32,views:&[colmap_model::View],
 alignment:colmap_model::Alignment,footprints:&BTreeMap<String,tiles::Window>,policy:Policy)->Result<Self,pipeline::Error>{
  if !root.is_absolute()||width==0||height==0||views.is_empty()||views.len()>100_000||
  policy.max_compressed_bytes==0||policy.max_compressed_bytes>256*1024*1024||
  policy.cache_bytes>2*1024*1024*1024||policy.max_decode_bytes==0||policy.max_decode_bytes>2*1024*1024*1024||
  policy.header.max_header_bytes==0||policy.header.max_header_bytes>4*1024*1024{return Err(pipeline::Error::Invalid);}
  let root=root.canonicalize()?;let mut all=Vec::with_capacity(views.len());let mut seen=std::collections::BTreeSet::new();
  for v in views{
   if !colmap_model::safe_name(&v.name)||!seen.insert(v.name.clone()){return Err(pipeline::Error::Invalid);}
   policy.header.check(images::Dimensions{width:v.width,height:v.height}).map_err(|_|pipeline::Error::Limit)?;
   let path=root.join(&v.name);let m=fs::symlink_metadata(&path)?;
   if !m.is_file()||m.file_type().is_symlink()||m.len()==0||m.len()>policy.max_compressed_bytes{return Err(pipeline::Error::Invalid);}
   let path=path.canonicalize()?;if !path.starts_with(&root){return Err(pipeline::Error::Invalid);}
   let camera=colmap_model::align(v,alignment).map_err(|_|pipeline::Error::Invalid)?;
   let footprint=footprints.get(&v.name).copied();
   if let Some(f)=footprint{if f.width==0||f.height==0||f.x.checked_add(f.width).is_none()||f.y.checked_add(f.height).is_none(){return Err(pipeline::Error::Invalid);}}
   all.push(View{path,camera,width:v.width,height:v.height,footprint});
  }
  if footprints.keys().any(|k|!seen.contains(k)){return Err(pipeline::Error::Invalid);}
  // Stable source order ensures deterministic optical-axis tie breaking.
  all.sort_by(|a,b|a.path.cmp(&b.path));
  Ok(Self{dem:pipeline::RawSource{width,height,dem_path,images:vec![]},views:all,policy,cache:image_cache::Cache::new(policy.cache_bytes)})
 }
}
impl pipeline::Source for Source{
 fn dem(&mut self,w:tiles::Window,max_bytes:u64)->Result<Vec<f32>,pipeline::Error>{
  pipeline::Source::dem(&mut self.dem,w,max_bytes)
 }
 fn images(&mut self,w:tiles::Window,max_bytes:u64)->Result<Vec<pipeline::OwnedImage>,pipeline::Error>{
  self.cache.trim_to(max_bytes);
  let mut held=0u64;let mut out=Vec::new();
  for v in &self.views{
   if v.footprint.is_some_and(|f|!pipeline::intersects(f,w)){continue;}
   if out.len()>=256{return Err(pipeline::Error::Limit);}
   let n=u64::from(v.width)*u64::from(v.height);
   let rgb=n.checked_mul(3).ok_or(pipeline::Error::Limit)?;
   let remaining=max_bytes.checked_sub(held).ok_or(pipeline::Error::Limit)?;
   if rgb>remaining{return Err(pipeline::Error::Limit);}
   if let Some(pixels)=self.cache.take(&v.path){
    if pixels.len() as u64!=rgb{return Err(pipeline::Error::Invalid);}
    self.cache.trim_to(remaining-rgb);
    held+=rgb;out.push(pipeline::OwnedImage{camera:v.camera,width:v.width,height:v.height,rgb:pixels});
    continue;
   }
   let f=File::open(&v.path)?;let meta=f.metadata()?;let len=meta.len();
   if !meta.is_file()||len==0||len>self.policy.max_compressed_bytes{return Err(pipeline::Error::Limit);}
   let remaining=max_bytes.checked_sub(held).ok_or(pipeline::Error::Limit)?;
   let decode_budget=remaining.min(self.policy.max_decode_bytes);
   let estimate=n.checked_mul(16).and_then(|n|n.checked_add(len)).ok_or(pipeline::Error::Limit)?;
   if estimate>decode_budget||rgb>remaining{return Err(pipeline::Error::Limit);}
   self.cache.trim_to(remaining-estimate);
   let mut bytes=Vec::new();f.take(len+1).read_to_end(&mut bytes)?;
   if bytes.len() as u64!=len{return Err(pipeline::Error::Invalid);}
   let decoded=jpeg::decode(&bytes,self.policy.header,decode_budget).map_err(pipeline::Error::Jpeg)?;
   if decoded.dimensions!=(images::Dimensions{width:v.width,height:v.height}){return Err(pipeline::Error::Invalid);}
   held=held.checked_add(rgb).ok_or(pipeline::Error::Limit)?;
   out.push(pipeline::OwnedImage{camera:v.camera,width:v.width,height:v.height,rgb:Arc::from(decoded.pixels)});
  }
  // Reinsert shared buffers only after all per-tile allocations are complete.
  // The host drops each returned tile batch before requesting another.
  for (v,im) in self.views.iter().filter(|v|!v.footprint.is_some_and(|f|!pipeline::intersects(f,w))).zip(&out){
   self.cache.insert(v.path.clone(),Arc::clone(&im.rgb));
  }
  Ok(out)
 }
}
#[cfg(test)]mod tests{
 use super::*;use super::pipeline::Source as _;
 struct Temp(PathBuf);impl Drop for Temp{fn drop(&mut self){let _=fs::remove_dir_all(&self.0);}}
 fn fixture()->(Temp,Source){
 let t=Temp(std::env::temp_dir().join(format!("ovg-jpeg-source-{}",uuid::Uuid::new_v4())));
 fs::create_dir(&t.0).unwrap();fs::write(t.0.join("a.jpg"),jpeg::tests::fixture()).unwrap();
 let dem=t.0.join("dem.raw");fs::write(&dem,[0u8;4]).unwrap();
 let views=colmap_model::parse("1 PINHOLE 2 2 10 10 0 0\n","1 0 1 0 0 0 0 10 1 a.jpg\n\n",10).unwrap();
 let alignment=colmap_model::Alignment{scale:1.,rotation:[[1.,0.,0.],[0.,1.,0.],[0.,0.,1.]],offset:[0.;3]};
 let s=Source::from_model(&t.0,dem,1,1,&views,alignment,&BTreeMap::new(),Policy{
 max_compressed_bytes:4096,max_decode_bytes:4096,cache_bytes:1024,header:images::Limits{max_header_bytes:1024,max_side:32,max_pixels:1024}}).unwrap();
 (t,s)
 }
 #[test]fn jpeg_camera_dem_to_file_tile(){
 let(t,mut source)=fixture();let store=super::super::disk_store::Store::open(&t.0.join("store")).unwrap();
 let grid=ortho::Grid{origin_x:0.,origin_y:0.,gsd:1.,width:1,height:1};
 let p=pipeline::Policy{tiles:tiles::Policy{min_side:1,max_side:1,halo_px:0,bytes_per_pixel:64,
 max_working_bytes:1024,max_tiles:2,max_depth:2},source_bytes:4096,tile_bytes:1024,max_samples:100,output_bytes:1024};
 let result=pipeline::run(&mut source,&store,0,"grid",32648,grid,p,||false).unwrap();
 assert_eq!(result.covered,1);assert_eq!(store.head().unwrap(),1);
 }
 #[test]fn repeated_tiles_reuse_decoded_buffer(){
 let(_t,mut source)=fixture();let w=tiles::Window{x:0,y:0,width:1,height:1};
 let first=source.images(w,4096).unwrap();let ptr=first[0].rgb.as_ptr();drop(first);
 let second=source.images(w,4096).unwrap();assert_eq!(ptr,second[0].rgb.as_ptr());
 }
 #[test]fn source_rejects_decode_budget(){
 let(_t,mut source)=fixture();let w=tiles::Window{x:0,y:0,width:1,height:1};
 assert!(matches!(source.images(w,10),Err(pipeline::Error::Limit)));}
}
