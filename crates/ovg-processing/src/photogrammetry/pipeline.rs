//! Sequential tile-native reference orthomosaic pipeline with a real disk sink.
//! Host supplies a qualified projected CRS, undistorted RGB and aligned DEM.
//! Source adapters are trusted; this is not an untrusted path or IPC interface.
use super::{disk_store,geotiff,ortho,tiles};
use std::{collections::BTreeMap,fs::File,io::{Read,Seek,SeekFrom},path::PathBuf,sync::Arc};

#[derive(Debug)]
pub enum Error{Invalid,Limit,Cancelled,Io(std::io::Error),Jpeg(super::jpeg::Error),Tile(tiles::Error),
 Render(ortho::Error),Tiff(geotiff::Error),Store(disk_store::Error)}
impl From<std::io::Error> for Error{fn from(e:std::io::Error)->Self{Self::Io(e)}}
pub struct OwnedImage{pub camera:ortho::Camera,pub width:u32,pub height:u32,pub rgb:Arc<[u8]>}
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
 grid:ortho::Grid,epsg:u16,p:Policy,cancelled:impl FnMut()->bool)->Result<ResultSummary,Error>{
 run_inner(source,store,expected,grid_id,grid,epsg,p,None,None,cancelled)
}
/// The host supplies a trusted digest covering actual source imagery, camera poses,
/// DEM, alignment, renderer/provider build and immutable input revision.
pub fn run_resumable(source:&mut impl Source,store:&disk_store::Store,expected:u64,grid_id:&str,
 grid:ortho::Grid,epsg:u16,p:Policy,input_sha256:&str,cancelled:impl FnMut()->bool)->Result<ResultSummary,Error>{
 if !super::digest(input_sha256){return Err(Error::Invalid);}
 run_inner(source,store,expected,grid_id,grid,epsg,p,Some(input_sha256),None,cancelled)
}
/// Parallel pixel rendering inside one admitted tile; source/cache and disk publication remain sequential.
pub fn run_parallel(source:&mut impl Source,store:&disk_store::Store,expected:u64,grid_id:&str,
 grid:ortho::Grid,epsg:u16,p:Policy,input_sha256:Option<&str>,threads:usize,cancel:&std::sync::atomic::AtomicBool)->Result<ResultSummary,Error>{
 if input_sha256.is_some_and(|h|!super::digest(h)){return Err(Error::Invalid);}
 let renderer=ortho::Renderer::new(threads).map_err(Error::Render)?;
 run_inner(source,store,expected,grid_id,grid,epsg,p,input_sha256,Some((&renderer,cancel)),||cancel.load(std::sync::atomic::Ordering::Acquire))
}
fn run_inner(source:&mut impl Source,store:&disk_store::Store,expected:u64,grid_id:&str,
 grid:ortho::Grid,epsg:u16,p:Policy,input_sha256:Option<&str>,parallel:Option<(&ortho::Renderer,&std::sync::atomic::AtomicBool)>,mut cancelled:impl FnMut()->bool)->Result<ResultSummary,Error>{
 if !super::valid_id(grid_id)||epsg==0||epsg>=32767||grid.width==0||grid.height==0||!grid.gsd.is_finite()||
 grid.gsd<=0.||!grid.origin_x.is_finite()||!grid.origin_y.is_finite()||
 p.source_bytes==0||p.source_bytes>2*1024*1024*1024||p.tile_bytes<308||p.output_bytes==0||
 p.max_samples==0||p.tiles.bytes_per_pixel<16||p.tiles.max_tiles>100_000{return Err(Error::Invalid);}
 if store.head().map_err(Error::Store)?!=expected{return Err(Error::Store(disk_store::Error::Conflict));}
 let ts=tiles::partition(tiles::Window{x:0,y:0,width:grid.width,height:grid.height},tiles::Layer::Compute,p.tiles).map_err(Error::Tile)?;
 let checkpoint=input_sha256.map(|hash|super::checkpoint::Checkpoint::open(store,expected,grid_id,grid,epsg,p,hash))
    .transpose().map_err(Error::Store)?;
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
 let id=format!("L{}_M{}",t.id.level,t.id.morton);
 if let Some(cp)=&checkpoint{
    if let Some(record)=cp.load(store,&id,c,tiff_bytes).map_err(Error::Store)?{
        covered=covered.checked_add(record.covered).ok_or(Error::Limit)?;
        refs.insert(id,record.tile);continue;
    }
 }
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
 }else if let Some((renderer,cancel))=parallel{
   renderer.render(subgrid(grid,h),&dem,&views,expected_rgba,p.max_samples,cancel)
    .map_err(|e|if e==ortho::Error::Cancelled{Error::Cancelled}else{Error::Render(e)})?
 }else{ortho::render(subgrid(grid,h),&dem,&views,expected_rgba,p.max_samples).map_err(Error::Render)?};
 let mut core=Vec::with_capacity(core_bytes as usize);
 for row in 0..c.height as usize{
   let start=((row+(c.y-h.y) as usize)*h.width as usize+(c.x-h.x) as usize)*4;
   core.extend_from_slice(&rgba[start..start+c.width as usize*4]);
 }
 let tile_covered=super::simd::opaque_rgba(&core);
 covered+=tile_covered;
 drop(rgba);drop(views);drop(imgs);drop(dem);
 let bytes=geotiff::rgba(subgrid(grid,c),epsg,&core,p.tile_bytes).map_err(Error::Tiff)?;
 let (sha256,bytes)=store.put(&bytes).map_err(Error::Store)?;
 let tile=disk_store::TileRef{window:c,sha256,bytes};
 if let Some(cp)=&checkpoint{
    cp.save(store,&id,super::checkpoint::Record{tile:tile.clone(),covered:tile_covered}).map_err(Error::Store)?;
 }
 refs.insert(id,tile);
 }
 if cancelled(){return Err(Error::Cancelled);}
 if covered==0{return Err(Error::Invalid);}
 let revision=store.publish(expected,grid_id,epsg,refs).map_err(Error::Store)?;
 Ok(ResultSummary{revision,tiles:ts.len(),pixels:u64::from(grid.width)*u64::from(grid.height),covered,written_bytes:written})
}

/// Multi-tile CPU rendering in bounded batches. The host reserves total memory
/// and threads before invoking this entry point; source adapters remain trusted.
/// Source I/O is sequential, render tasks run in one dedicated pool, publication
/// is serial and deterministic. No full-mosaic allocation.
#[derive(Debug,Clone,Copy)]
pub struct BatchPolicy{pub threads:usize,pub max_in_flight:usize,pub host_bytes:u64}
struct PreparedTile{tile:tiles::Tile,id:String,dem:Vec<f32>,images:Vec<OwnedImage>}
fn tile_cost(t:&tiles::Tile,p:Policy)->Result<(u64,u64),Error>{
 let halo=u64::from(t.halo.width).checked_mul(u64::from(t.halo.height)).ok_or(Error::Limit)?;
 let core=u64::from(t.core.width).checked_mul(u64::from(t.core.height)).ok_or(Error::Limit)?;
 let tiff=core.checked_mul(4).and_then(|n|n.checked_add(304)).ok_or(Error::Limit)?;
 let work=halo.checked_mul(8).and_then(|n|n.checked_add(core.checked_mul(4)?)).and_then(|n|n.checked_add(tiff)).ok_or(Error::Limit)?;
 if work>p.tiles.max_working_bytes||tiff>p.tile_bytes||tiff>256*1024*1024{return Err(Error::Limit);}
 // Include source budget and one row scratch; adapter metadata/allocator overhead
 // is covered by separate host qualification, not claimed as measured RSS.
 let charge=work.checked_add(p.source_bytes).and_then(|n|n.checked_add(u64::from(t.halo.width)*4)).ok_or(Error::Limit)?;
 Ok((charge,tiff))
}
fn prepare_tile(source:&mut impl Source,t:tiles::Tile,id:String,p:Policy)->Result<PreparedTile,Error>{
 let n=u64::from(t.halo.width)*u64::from(t.halo.height);
 let dem=source.dem(t.halo,n.checked_mul(4).ok_or(Error::Limit)?)?;
 if dem.len() as u64!=n{return Err(Error::Invalid);}
 let images=source.images(t.halo,p.source_bytes)?;
 let used=images.iter().try_fold(0u64,|n,i|n.checked_add(i.rgb.len() as u64)).ok_or(Error::Limit)?;
 if used>p.source_bytes||images.len()>256{return Err(Error::Limit);}
 Ok(PreparedTile{tile:t,id,dem,images})
}
fn render_prepared(t:PreparedTile,g:ortho::Grid,epsg:u16,p:Policy,cancel:&std::sync::atomic::AtomicBool)->Result<(String,tiles::Window,u64,Vec<u8>),Error>{
 use std::sync::atomic::Ordering;
 if cancel.load(Ordering::Acquire){return Err(Error::Cancelled);}
 let h=t.tile.halo;let c=t.tile.core;
 let n=u64::from(h.width)*u64::from(h.height)*4;
 let views:Vec<_>=t.images.iter().map(|i|ortho::Image{camera:i.camera,width:i.width,height:i.height,rgb:&i.rgb}).collect();
 let rgba=if views.is_empty(){if n>256*1024*1024{return Err(Error::Limit);}vec![0;n as usize]}
 else{ortho::render(subgrid(g,h),&t.dem,&views,n,p.max_samples).map_err(Error::Render)?};
 if cancel.load(Ordering::Acquire){return Err(Error::Cancelled);}
 let mut core=Vec::with_capacity(c.width as usize*c.height as usize*4);
 for row in 0..c.height as usize{
 let start=((row+(c.y-h.y) as usize)*h.width as usize+(c.x-h.x) as usize)*4;
 core.extend_from_slice(&rgba[start..start+c.width as usize*4]);
 }
 let covered=super::simd::opaque_rgba(&core);
 drop(rgba);drop(views);drop(t.images);drop(t.dem);
 let bytes=geotiff::rgba(subgrid(g,c),epsg,&core,p.tile_bytes).map_err(Error::Tiff)?;
 Ok((t.id,c,covered,bytes))
}
pub fn run_multitile(source:&mut impl Source,store:&disk_store::Store,expected:u64,grid_id:&str,
 grid:ortho::Grid,epsg:u16,p:Policy,b:BatchPolicy,input_sha256:Option<&str>,
 cancel:&std::sync::atomic::AtomicBool)->Result<ResultSummary,Error>{
 use rayon::prelude::*;
 use std::sync::atomic::Ordering;
 if !super::valid_id(grid_id)||epsg==0||epsg>=32767||grid.width==0||grid.height==0||
 !grid.gsd.is_finite()||grid.gsd<=0.||!grid.origin_x.is_finite()||!grid.origin_y.is_finite()||
 p.source_bytes==0||p.source_bytes>2*1024*1024*1024||p.tile_bytes<308||p.output_bytes==0||
 p.max_samples==0||p.tiles.bytes_per_pixel<16||p.tiles.max_tiles>100_000||
 b.threads==0||b.threads>32||b.max_in_flight==0||b.max_in_flight>b.threads||
 input_sha256.is_some_and(|v|!super::digest(v)){return Err(Error::Invalid);}
 if store.head().map_err(Error::Store)?!=expected{return Err(Error::Store(disk_store::Error::Conflict));}
 let ts=tiles::partition(tiles::Window{x:0,y:0,width:grid.width,height:grid.height},tiles::Layer::Compute,p.tiles).map_err(Error::Tile)?;
 // Reserve space for persistent source cache, thread stacks and bounded metadata.
 let fixed=p.source_bytes.checked_add((b.threads as u64)*2*1024*1024)
 .and_then(|n|n.checked_add(ts.len() as u64*1024)).ok_or(Error::Limit)?;
 let available=b.host_bytes.checked_sub(fixed).ok_or(Error::Limit)?;
 let mut total=0u64;
 for t in &ts{let(charge,bytes)=tile_cost(t,p)?;if charge>available{return Err(Error::Limit);}
 total=total.checked_add(bytes).ok_or(Error::Limit)?;if total>p.output_bytes{return Err(Error::Limit);}}
 let pool=rayon::ThreadPoolBuilder::new().num_threads(b.threads).stack_size(2*1024*1024).build().map_err(|_|Error::Limit)?;
 let checkpoint=input_sha256.map(|hash|super::checkpoint::Checkpoint::open(store,expected,grid_id,grid,epsg,p,hash))
 .transpose().map_err(Error::Store)?;
 let mut refs=BTreeMap::new();let mut covered=0u64;let mut i=0usize;
 while i<ts.len(){
 if cancel.load(Ordering::Acquire){return Err(Error::Cancelled);}
 let mut batch=Vec::new();let mut used=0u64;
 while i<ts.len()&&batch.len()<b.max_in_flight{
 if cancel.load(Ordering::Acquire){return Err(Error::Cancelled);}
 let t=ts[i];let(charge,bytes)=tile_cost(&t,p)?;
 let id=format!("L{}_M{}",t.id.level,t.id.morton);
 if let Some(cp)=&checkpoint{if let Some(record)=cp.load(store,&id,t.core,bytes).map_err(Error::Store)?{
 covered=covered.checked_add(record.covered).ok_or(Error::Limit)?;refs.insert(id,record.tile);i+=1;continue;
 }}
 let next=used.checked_add(charge).ok_or(Error::Limit)?;
 if next>available{break;}
 batch.push(prepare_tile(source,t,id,p)?);used=next;i+=1;
 }
 if batch.is_empty(){if i==ts.len(){break;}return Err(Error::Limit);}
 let results:Vec<_>=pool.install(||batch.into_par_iter().map(|t|render_prepared(t,grid,epsg,p,cancel)).collect());
 for result in results{
 if cancel.load(Ordering::Acquire){return Err(Error::Cancelled);}
 let(id,window,n,data)=result?;let(sha256,bytes)=store.put(&data).map_err(Error::Store)?;
 let tile=disk_store::TileRef{window,sha256,bytes};
 if let Some(cp)=&checkpoint{cp.save(store,&id,super::checkpoint::Record{tile:tile.clone(),covered:n}).map_err(Error::Store)?;}
 covered=covered.checked_add(n).ok_or(Error::Limit)?;refs.insert(id,tile);
 }
 }
 if cancel.load(Ordering::Acquire){return Err(Error::Cancelled);}
 if covered==0||refs.len()!=ts.len(){return Err(Error::Invalid);}
 let revision=store.publish(expected,grid_id,epsg,refs).map_err(Error::Store)?;
 Ok(ResultSummary{revision,tiles:ts.len(),pixels:u64::from(grid.width)*u64::from(grid.height),covered,written_bytes:total})
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
 out.push(OwnedImage{camera:im.camera,width:im.width,height:im.height,rgb:rgb.into()});
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
 #[test]fn parallel_pipeline_matches_serial_digests(){
 let(t,mut source,g)=fixture();let a=disk_store::Store::open(&t.0.join("a")).unwrap();let b=disk_store::Store::open(&t.0.join("b")).unwrap();
 let ra=run(&mut source,&a,0,"grid",32648,g,policy(),||false).unwrap();
 let cancel=std::sync::atomic::AtomicBool::new(false);
 let rb=run_parallel(&mut source,&b,0,"grid",32648,g,policy(),None,2,&cancel).unwrap();
 assert_eq!(ra,rb);assert_eq!(a.revision(1).unwrap().tiles,b.revision(1).unwrap().tiles);
 }
 #[test]fn multitile_batch_matches_serial_and_checks_budget(){
 let(t,mut source,g)=fixture();let a=disk_store::Store::open(&t.0.join("a")).unwrap();let b=disk_store::Store::open(&t.0.join("b")).unwrap();
 let ra=run(&mut source,&a,0,"grid",32648,g,policy(),||false).unwrap();
 let cancel=std::sync::atomic::AtomicBool::new(false);
 let bp=BatchPolicy{threads:2,max_in_flight:2,host_bytes:8*1024*1024};
 assert!(matches!(run_multitile(&mut source,&b,0,"grid",32648,g,policy(),BatchPolicy{host_bytes:1,..bp},None,&cancel),Err(Error::Limit)));
 assert_eq!(b.head().unwrap(),0);
 let rb=run_multitile(&mut source,&b,0,"grid",32648,g,policy(),bp,None,&cancel).unwrap();
 assert_eq!(ra,rb);assert_eq!(a.revision(1).unwrap().tiles,b.revision(1).unwrap().tiles);
 }
 #[test]fn resumes_verified_tile_after_cancel(){
 let(t,mut source,g)=fixture();let s=disk_store::Store::open(&t.0.join("store")).unwrap();
 let hash="a".repeat(64);let mut calls=0;
 let r=run_resumable(&mut source,&s,0,"grid",32648,g,policy(),&hash,||{calls+=1;calls>1});
 assert!(matches!(r,Err(Error::Cancelled)));assert_eq!(s.head().unwrap(),0);
 let r=run_resumable(&mut source,&s,0,"grid",32648,g,policy(),&hash,||false).unwrap();
 assert_eq!(r.covered,9);assert_eq!(r.tiles,4);assert_eq!(s.head().unwrap(),1);
 }
 #[test]fn cancelled_does_not_publish(){
 let(t,mut source,g)=fixture();let s=disk_store::Store::open(&t.0.join("store")).unwrap();
 let mut calls=0;let r=run(&mut source,&s,0,"grid",32648,g,policy(),||{calls+=1;calls>1});
 assert!(matches!(r,Err(Error::Cancelled)));assert_eq!(s.head().unwrap(),0);
 }
}
