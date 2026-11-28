//! Sequential tile-native reference orthomosaic pipeline with a real disk sink.
//! Host supplies a qualified projected CRS, undistorted RGB and aligned DEM.
//! Source adapters are trusted; this is not an untrusted path or IPC interface.
use super::{disk_store,geotiff,ortho,tiles};
use std::{collections::BTreeMap,fs::File,io::{Read,Seek,SeekFrom},path::PathBuf};

#[derive(Debug)]
pub enum Error{Invalid,Limit,Cancelled,Io(std::io::Error),Jpeg(super::jpeg::Error),Tile(tiles::Error),
 Render(ortho::Error),Tiff(geotiff::Error),Store(disk_store::Error)}
impl From<std::io::Error> for Error{fn from(e:std::io::Error)->Self{Self::Io(e)}}
pub struct OwnedImage{pub camera:ortho::Camera,pub width:u32,pub height:u32,pub rgb:Vec<u8>}
pub trait Source{
 fn dem(&mut self,window:tiles::Window,max_bytes:u64)->Result<Vec<f32>,Error>;
 fn images(&mut self,window:tiles::Window,max_bytes:u64)->Result<Vec<OwnedImage>,Error>;
}
#[derive(Debug,Clone,Copy)]
pub struct Policy{pub tiles:tiles::Policy,pub source_bytes:u64,pub tile_bytes:u64,pub max_samples:u64,pub output_bytes:u64}
#[derive(Debug,Clone,Copy,PartialEq,Eq)]
pub struct ResultSummary{pub revision:u64,pub tiles:usize,pub pixels:u64,pub covered:u64,pub written_bytes:u64}
fn subgrid(g:ortho::Grid,w:tiles::Window)->ortho::Grid{
 ortho::Grid{origin_x:g.origin_x+f64::from(w.x)*g.gsd,origin_y:g.origin_y-f64::from(w.y)*g.gsd,gsd:g.gsd,width:w.width,height:w.height}
}
/// Render one tile at a time. Halo is rendered, then the core is cropped.
/// Failure/cancellation leaves previous revision unchanged; unreferenced immutable
/// objects may remain on disk until a future lease-aware garbage collector.
pub fn run(source:&mut impl Source,store:&disk_store::Store,expected:u64,grid_id:&str,
 grid:ortho::Grid,epsg:u16,p:Policy,mut cancelled:impl FnMut()->bool)->Result<ResultSummary,Error>{
 if !super::valid_id(grid_id)||epsg==0||epsg>=32767||grid.width==0||grid.height==0||!grid.gsd.is_finite()||
 grid.gsd<=0.||!grid.origin_x.is_finite()||!grid.origin_y.is_finite()||
 p.source_bytes==0||p.source_bytes>2*1024*1024*1024||p.tile_bytes<308||p.output_bytes==0||
 p.max_samples==0||p.tiles.bytes_per_pixel<16||p.tiles.max_tiles>100_000{return Err(Error::Invalid);}
 if store.head().map_err(Error::Store)?!=expected{return Err(Error::Store(disk_store::Error::Conflict));}
 let ts=tiles::partition(tiles::Window{x:0,y:0,width:grid.width,height:grid.height},tiles::Layer::Compute,p.tiles).map_err(Error::Tile)?;
 let mut refs=BTreeMap::new();let mut covered=0u64;let mut written=0u64;
 for t in &ts{
 if cancelled(){return Err(Error::Cancelled);}
 let h=t.halo;let c=t.core;
 let halo_pixels=u64::from(h.width)*u64::from(h.height);
 let core_pixels=u64::from(c.width)*u64::from(c.height);
 let expected_dem=halo_pixels.checked_mul(4).ok_or(Error::Limit)?;
 let expected_rgba=expected_dem;
 let core_bytes=core_pixels.checked_mul(4).ok_or(Error::Limit)?;
 let tiff_bytes=core_bytes.checked_add(304).ok_or(Error::Limit)?;
 let working=expected_dem.checked_add(expected_rgba).and_then(|n|n.checked_add(core_bytes))
    .and_then(|n|n.checked_add(tiff_bytes)).ok_or(Error::Limit)?;
 if working>p.tiles.max_working_bytes||tiff_bytes>p.tile_bytes{return Err(Error::Limit);}
 written=written.checked_add(tiff_bytes).ok_or(Error::Limit)?;
 if written>p.output_bytes{return Err(Error::Limit);}
 let dem=source.dem(h,expected_dem)?;
 if dem.len() as u64!=halo_pixels{return Err(Error::Invalid);}
 let imgs=source.images(h,p.source_bytes)?;
 if imgs.len()>256{return Err(Error::Limit);}
 let mut source_bytes=0u64;
 for im in &imgs{source_bytes=source_bytes.checked_add(im.rgb.len() as u64).ok_or(Error::Limit)?;}
 if source_bytes>p.source_bytes{return Err(Error::Limit);}
 let views:Vec<_>=imgs.iter().map(|im|ortho::Image{camera:im.camera,width:im.width,height:im.height,rgb:&im.rgb}).collect();
 let rgba=if views.is_empty(){
   if expected_rgba>256*1024*1024{return Err(Error::Limit);}vec![0;expected_rgba as usize]
 }else{ortho::render(subgrid(grid,h),&dem,&views,expected_rgba,p.max_samples).map_err(Error::Render)?};
 let mut core=Vec::with_capacity(core_bytes as usize);
 for row in 0..c.height as usize{
   let start=((row+(c.y-h.y) as usize)*h.width as usize+(c.x-h.x) as usize)*4;
   core.extend_from_slice(&rgba[start..start+c.width as usize*4]);
 }
 covered+=core.chunks_exact(4).filter(|q|q[3]==255).count() as u64;
 drop(rgba);drop(views);drop(imgs);drop(dem);
 let bytes=geotiff::rgba(subgrid(grid,c),epsg,&core,p.tile_bytes).map_err(Error::Tiff)?;
 let (sha256,bytes)=store.put(&bytes).map_err(Error::Store)?;
 refs.insert(format!("L{}_M{}",t.id.level,t.id.morton),disk_store::TileRef{window:c,sha256,bytes});
 }
 if cancelled(){return Err(Error::Cancelled);}
 if covered==0{return Err(Error::Invalid);}
 let revision=store.publish(expected,grid_id,epsg,refs).map_err(Error::Store)?;
 Ok(ResultSummary{revision,tiles:ts.len(),pixels:u64::from(grid.width)*u64::from(grid.height),covered,written_bytes:written})
}

/// Bounded raw source adapter: little-endian f32 DEM and packed undistorted RGB.
/// A practical input for upstream qualified decoders; not a JPEG/EXIF reader.
pub struct RawImage{pub path:PathBuf,pub camera:ortho::Camera,pub width:u32,pub height:u32,
 pub footprint:Option<tiles::Window>}
pub struct RawSource{pub width:u32,pub height:u32,pub dem_path:PathBuf,pub images:Vec<RawImage>}
pub(crate) fn intersects(a:tiles::Window,b:tiles::Window)->bool{
 match(a.x.checked_add(a.width),a.y.checked_add(a.height),b.x.checked_add(b.width),b.y.checked_add(b.height)){
 (Some(ax),Some(ay),Some(bx),Some(by))=>a.width>0&&a.height>0&&b.width>0&&b.height>0&&a.x<bx&&b.x<ax&&a.y<by&&b.y<ay,
 _=>false}
}
fn exact_file(path:&PathBuf,bytes:u64)->Result<File,Error>{
 let f=File::open(path)?;let meta=f.metadata()?;if !meta.is_file()||meta.len()!=bytes{return Err(Error::Invalid);}Ok(f)
}
impl Source for RawSource{
 fn dem(&mut self,w:tiles::Window,max_bytes:u64)->Result<Vec<f32>,Error>{
 let xend=w.x.checked_add(w.width).ok_or(Error::Invalid)?;let yend=w.y.checked_add(w.height).ok_or(Error::Invalid)?;
 if w.width==0||w.height==0||xend>self.width||yend>self.height{return Err(Error::Invalid);}
 let n=u64::from(w.width)*u64::from(w.height);let len=n.checked_mul(4).ok_or(Error::Limit)?;
 if len>max_bytes||len>256*1024*1024{return Err(Error::Limit);}
 let full=u64::from(self.width).checked_mul(u64::from(self.height)).and_then(|n|n.checked_mul(4)).ok_or(Error::Limit)?;
 let mut f=exact_file(&self.dem_path,full)?;let mut out=Vec::with_capacity(n as usize);let mut row=vec![0;w.width as usize*4];
 for y in w.y..yend{
  let offset=(u64::from(y)*u64::from(self.width)+u64::from(w.x))*4;f.seek(SeekFrom::Start(offset))?;
  f.read_exact(&mut row)?;out.extend(row.chunks_exact(4).map(|c|f32::from_le_bytes([c[0],c[1],c[2],c[3]])));
 }
 Ok(out)
 }
 fn images(&mut self,w:tiles::Window,max_bytes:u64)->Result<Vec<OwnedImage>,Error>{
 if self.images.len()>100_000{return Err(Error::Limit);}
 let mut total=0u64;let mut out=Vec::new();
 for im in &self.images{
 if let Some(f)=im.footprint{
  if f.width==0||f.height==0||f.x.checked_add(f.width).is_none()||f.y.checked_add(f.height).is_none(){return Err(Error::Invalid);}
  if !intersects(f,w){continue;}
 }
 let len=u64::from(im.width).checked_mul(u64::from(im.height)).and_then(|n|n.checked_mul(3)).ok_or(Error::Limit)?;
 total=total.checked_add(len).ok_or(Error::Limit)?;
 if total>max_bytes||len>256*1024*1024||out.len()>=256{return Err(Error::Limit);}
 if len==0||!im.camera.valid(){return Err(Error::Invalid);}
 let mut f=exact_file(&im.path,len)?;let mut rgb=vec![0;len as usize];f.read_exact(&mut rgb)?;
 let mut extra=[0];if f.read(&mut extra)?!=0{return Err(Error::Invalid);}
 out.push(OwnedImage{camera:im.camera,width:im.width,height:im.height,rgb});
 }
 Ok(out)
 }
}
#[cfg(test)]mod tests{
 use super::*;use std::fs;use uuid::Uuid;
 struct Temp(PathBuf);impl Drop for Temp{fn drop(&mut self){let _=fs::remove_dir_all(&self.0);}}
 fn fixture()->(Temp,RawSource,ortho::Grid){
 let t=Temp(std::env::temp_dir().join(format!("ovg-pipe-{}",Uuid::new_v4())));fs::create_dir_all(&t.0).unwrap();
 let dem=t.0.join("dem.raw");fs::write(&dem,vec![0u8;3*3*4]).unwrap();
 let rgb=t.0.join("rgb.raw");let data:Vec<u8>=(0..4).flat_map(|y|(0..4).flat_map(move|x|[x*40,y*40,0])).collect();fs::write(&rgb,data).unwrap();
 let camera=ortho::Camera{world_to_camera:[[1.,0.,0.],[0.,-1.,0.],[0.,0.,-1.]],center:[0.,0.,10.],fx:10.,fy:10.,cx:0.,cy:0.};
 let source=RawSource{width:3,height:3,dem_path:dem,images:vec![RawImage{path:rgb,camera,width:4,height:4,footprint:None}]};
 let grid=ortho::Grid{origin_x:0.,origin_y:0.,gsd:1.,width:3,height:3};(t,source,grid)
 }
 fn policy()->Policy{Policy{tiles:tiles::Policy{min_side:1,max_side:2,halo_px:1,bytes_per_pixel:64,max_working_bytes:2048,max_tiles:16,max_depth:4},source_bytes:1024,tile_bytes:1024,max_samples:1000,output_bytes:8192}}
 #[test]fn tiled_file_output_matches_whole_frame(){
 let(t,mut source,g)=fixture();let s=disk_store::Store::open(&t.0.join("store")).unwrap();
 let r=run(&mut source,&s,0,"grid",32648,g,policy(),||false).unwrap();assert_eq!(r.covered,9);assert_eq!(r.tiles,4);
 let mut assembled=vec![0;36];
 for tile in s.revision(1).unwrap().tiles.values(){
 let b=fs::read(t.0.join("store/objects").join(&tile.sha256)).unwrap();let w=tile.window;
 for y in 0..w.height as usize{let dst=((w.y as usize+y)*3+w.x as usize)*4;let src=304+y*w.width as usize*4;
 assembled[dst..dst+w.width as usize*4].copy_from_slice(&b[src..src+w.width as usize*4]);}
 }
 let imgs=source.images(tiles::Window{x:0,y:0,width:3,height:3},1024).unwrap();
 let views:Vec<_>=imgs.iter().map(|im|ortho::Image{camera:im.camera,width:im.width,height:im.height,rgb:&im.rgb}).collect();
 assert_eq!(assembled,ortho::render(g,&[0.;9],&views,36,1000).unwrap());
 }
 #[test]fn cancelled_does_not_publish(){
 let(t,mut source,g)=fixture();let s=disk_store::Store::open(&t.0.join("store")).unwrap();
 let mut calls=0;let r=run(&mut source,&s,0,"grid",32648,g,policy(),||{calls+=1;calls>1});
 assert!(matches!(r,Err(Error::Cancelled)));assert_eq!(s.head().unwrap(),0);
 }
}
