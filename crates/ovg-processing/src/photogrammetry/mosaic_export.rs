//! Build an exact-grid GDAL VRT from verified immutable GeoTIFF shards.
//! VRT is not itself a COG; the host must qualify GDAL and independently inspect output.
use super::{disk_store,ortho::Grid,tiles::Window};
use std::{collections::{BTreeMap,BTreeSet},fs::{self,File},io::{Read,Write},path::{Path,PathBuf},ffi::OsString};
#[derive(Debug)]pub enum Error{Invalid,Limit,Overlap,Gap,Io(std::io::Error),Store(disk_store::Error)}
impl From<std::io::Error> for Error{fn from(e:std::io::Error)->Self{Self::Io(e)}}
fn validate_grid(g:Grid)->bool{g.width>0&&g.height>0&&g.origin_x.is_finite()&&g.origin_y.is_finite()&&g.gsd.is_finite()&&g.gsd>0.}
/// Sweep half-open Y events, maintaining disjoint X intervals. No raster-sized bitmap.
fn coverage(windows:&[Window],g:Grid)->Result<(),Error>{
 if !validate_grid(g)||windows.is_empty()||windows.len()>100_000{return Err(Error::Invalid);}
 let mut events:BTreeMap<u32,Vec<(bool,u32,u32)>>=BTreeMap::new();
 for w in windows{
 let x=w.x.checked_add(w.width).ok_or(Error::Invalid)?;let y=w.y.checked_add(w.height).ok_or(Error::Invalid)?;
 if w.width==0||w.height==0||x>g.width||y>g.height{return Err(Error::Invalid);}
 events.entry(w.y).or_default().push((true,w.x,x));events.entry(y).or_default().push((false,w.x,x));
 }
 let mut active=BTreeSet::<(u32,u32)>::new();let mut span=0u64;let mut prev=0u32;
 for(y,mut changes)in events{
 if y>prev&&span!=u64::from(g.width){return Err(Error::Gap);}
 changes.sort_unstable(); // removals before additions
 for(add,x,end)in changes{
 if add{
 if active.range(..=(x,u32::MAX)).next_back().is_some_and(|v|v.1>x)||
 active.range((x,0)..).next().is_some_and(|v|v.0<end){return Err(Error::Overlap);}
 if !active.insert((x,end)){return Err(Error::Overlap);}span+=u64::from(end-x);
 }else{if !active.remove(&(x,end)){return Err(Error::Invalid);}span-=u64::from(end-x);}
 }
 prev=y;
 }
 if prev!=g.height||!active.is_empty(){return Err(Error::Gap);}Ok(())
}
fn header(p:&Path,w:Window,g:Grid,epsg:u16,len:u64)->Result<(),Error>{
 let expect=u64::from(w.width).checked_mul(u64::from(w.height)).and_then(|n|n.checked_mul(4)).and_then(|n|n.checked_add(304)).ok_or(Error::Limit)?;
 if len!=expect||len>256*1024*1024{return Err(Error::Invalid);}
 let mut h=[0u8;304];File::open(p)?.read_exact(&mut h)?;
 // Generate the canonical header using a 1x1 tile, then patch dimensions and strip length.
 let sub=Grid{origin_x:g.origin_x+f64::from(w.x)*g.gsd,origin_y:g.origin_y-f64::from(w.y)*g.gsd,gsd:g.gsd,width:1,height:1};
 let mut expected=super::geotiff::rgba(sub,epsg,&[0;4],308).map_err(|_|Error::Invalid)?;
 expected[18..22].copy_from_slice(&w.width.to_le_bytes());
 expected[30..34].copy_from_slice(&w.height.to_le_bytes());
 expected[102..106].copy_from_slice(&w.height.to_le_bytes());
 expected[114..118].copy_from_slice(&((len-304) as u32).to_le_bytes());
 if h.as_slice()!=&expected[..304]{return Err(Error::Invalid);}Ok(())
}
/// Fresh host-owned directory. No source path text is interpolated into XML.
pub fn stage_vrt(store:&disk_store::Store,revision:u64,g:Grid,root:&Path,max_xml:u64)->Result<PathBuf,Error>{
 if !root.is_absolute()||!validate_grid(g)||max_xml==0||max_xml>256*1024*1024{return Err(Error::Invalid);}
 let r=store.revision(revision).map_err(Error::Store)?;
 let windows:Vec<_>=r.tiles.values().map(|v|v.window).collect();coverage(&windows,g)?;
 fs::create_dir(root)?;fs::create_dir(root.join("tiles"))?;
 for (i,t) in r.tiles.values().enumerate(){
 let p=store.verified_path(&t.sha256,t.bytes).map_err(Error::Store)?;
 header(&p,t.window,g,r.epsg,t.bytes)?;
 // Hard links preserve the content-addressed immutable source; host must enforce read-only access.
 fs::hard_link(p,root.join("tiles").join(format!("{i:06}.tif")))?;
 }
 let vrt=root.join("mosaic.vrt");let mut f=File::options().write(true).create_new(true).open(&vrt)?;
 let mut count=0u64;
 let mut append=|s:&str|->Result<(),Error>{count=count.checked_add(s.len() as u64).ok_or(Error::Limit)?;
 if count>max_xml{return Err(Error::Limit);}f.write_all(s.as_bytes())?;Ok(())};
 append(&format!("<VRTDataset rasterXSize=\"{}\" rasterYSize=\"{}\"><SRS>EPSG:{}</SRS><GeoTransform>{}, {}, 0, {}, 0, {}</GeoTransform>\n",g.width,g.height,r.epsg,g.origin_x,g.gsd,g.origin_y,-g.gsd))?;
 for (band,color)in [(1,"Red"),(2,"Green"),(3,"Blue"),(4,"Alpha")]{
 append(&format!("<VRTRasterBand dataType=\"Byte\" band=\"{band}\"><ColorInterp>{color}</ColorInterp>\n"))?;
 for(i,t)in r.tiles.values().enumerate(){let w=t.window;
 append(&format!("<SimpleSource><SourceFilename relativeToVRT=\"1\">tiles/{i:06}.tif</SourceFilename><SourceBand>{band}</SourceBand><SrcRect xOff=\"0\" yOff=\"0\" xSize=\"{}\" ySize=\"{}\"/><DstRect xOff=\"{}\" yOff=\"{}\" xSize=\"{}\" ySize=\"{}\"/></SimpleSource>\n",w.width,w.height,w.x,w.y,w.width,w.height))?;
 }
 append("</VRTRasterBand>\n")?;
 }
 append("</VRTDataset>\n")?;drop(append);f.sync_all()?;Ok(vrt)
}
/// Fixed local paths and options for an isolated GDAL 3.x worker; no shell strings.
pub fn cog_args(block:u16,threads:u8)->Result<Vec<OsString>,Error>{
 if !matches!(block,256|512|1024)||threads==0||threads>32{return Err(Error::Invalid);}
 Ok(vec!["-of".into(),"COG".into(),"-co".into(),"COMPRESS=DEFLATE".into(),"-co".into(),format!("BLOCKSIZE={block}").into(),
 "-co".into(),format!("NUM_THREADS={threads}").into(),"-co".into(),"BIGTIFF=IF_SAFER".into(),
 "-co".into(),"RESAMPLING=NEAREST".into(),"mosaic.vrt".into(),"mosaic.cog.tif".into()])
}
#[cfg(test)]mod tests{
 use super::*;
 fn grid()->Grid{Grid{origin_x:500000.,origin_y:1000000.,gsd:1.,width:2,height:1}}
 #[test]fn rejects_gaps_overlap(){let a=Window{x:0,y:0,width:1,height:1};let b=Window{x:1,..a};
 assert!(coverage(&[a,b],grid()).is_ok());assert!(matches!(coverage(&[a],grid()),Err(Error::Gap)));
 assert!(matches!(coverage(&[a,a,b],grid()),Err(Error::Overlap)));}
 #[test]fn verified_shards_to_vrt(){
 let dir=std::env::temp_dir().join(format!("ovg-vrt-{}",uuid::Uuid::new_v4()));let store=disk_store::Store::open(&dir).unwrap();
 let g=grid();let mut tiles=BTreeMap::new();
 for x in 0..2{let w=Window{x,y:0,width:1,height:1};let sub=Grid{origin_x:g.origin_x+f64::from(x),width:1,..g};
 let b=super::super::geotiff::rgba(sub,32648,&[10,20,30,255],1024).unwrap();
 let(sha256,bytes)=store.put(&b).unwrap();tiles.insert(format!("t{x}"),disk_store::TileRef{window:w,sha256,bytes});}
 store.publish(0,"grid",32648,tiles).unwrap();
 let vrt=stage_vrt(&store,1,g,&dir.join("export"),100000).unwrap();
 let s=fs::read_to_string(vrt).unwrap();assert_eq!(s.matches("<SimpleSource>").count(),8);
 assert!(s.contains("EPSG:32648"));assert!(cog_args(512,4).is_ok());let _=fs::remove_dir_all(dir);
 }
}
