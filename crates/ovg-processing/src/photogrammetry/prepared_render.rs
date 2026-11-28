//! Local prepared-image preview package importer. No raw flight reconstruction.
//! Host supplies native-picker directory and private fresh snapshot/output paths.
//! Package checksums prove consistency, not survey provenance or camera accuracy.
use super::{colmap_model,jpeg_source,images,ortho,pipeline,tiles,disk_store};
use serde::{Deserialize,Serialize};
use sha2::{Digest,Sha256};
use std::{collections::BTreeMap,fs::{self,File,OpenOptions},io::{Read,Write},path::Path};
#[derive(Debug)]pub enum Error{Invalid,Limit,Cancelled,Io(std::io::Error),Render(pipeline::Error)}
impl From<std::io::Error> for Error{fn from(e:std::io::Error)->Self{Self::Io(e)}}
#[derive(Deserialize)]#[serde(deny_unknown_fields)]
pub struct Manifest{pub version:u32,pub epsg:u16,pub origin_x:f64,pub origin_y:f64,pub gsd:f64,
 pub width:u32,pub height:u32,pub scale:f64,pub rotation:[[f64;3];3],pub offset:[f64;3],
 pub files:BTreeMap<String,String>}
#[derive(Debug,Clone,Serialize)]pub struct Receipt{pub revision:u64,pub tiles:usize,pub pixels:u64,pub covered:u64,pub written_bytes:u64,pub epsg:u16}
const MAX_INPUT:u64=2*1024*1024*1024;
fn read_text(p:&Path,max:u64)->Result<String,Error>{
 let m=fs::symlink_metadata(p)?;if !m.file_type().is_file()||m.len()>max{return Err(Error::Limit);}
 let mut s=String::new();File::open(p)?.take(max+1).read_to_string(&mut s)?;
 if s.len() as u64>max{return Err(Error::Limit);}Ok(s)
}
fn copy_verified(src:&Path,dst:&Path,hash:&str,max:u64,cancelled:&mut impl FnMut()->bool)->Result<u64,Error>{
 if !super::digest(hash){return Err(Error::Invalid);}
 let m=fs::symlink_metadata(src)?;if !m.file_type().is_file()||m.len()==0||m.len()>max{return Err(Error::Limit);}
 let mut f=File::open(src)?;let mut out=OpenOptions::new().write(true).create_new(true).open(dst)?;
 let mut h=Sha256::new();let mut b=[0;65536];let mut n=0u64;
 loop{if cancelled(){return Err(Error::Cancelled);}let k=f.read(&mut b)?;if k==0{break;}
 n=n.checked_add(k as u64).ok_or(Error::Limit)?;if n>max||n>m.len(){return Err(Error::Limit);}
 h.update(&b[..k]);out.write_all(&b[..k])?;}
 if n!=m.len()||format!("{:x}",h.finalize())!=hash{return Err(Error::Invalid);}
 out.sync_all()?;Ok(n)
}
fn allowed(name:&str)->bool{
 matches!(name,"cameras.txt"|"images.txt"|"dem.raw")||name.strip_prefix("images/").is_some_and(|n|
 colmap_model::safe_name(n)&&n.to_ascii_lowercase().ends_with(".jpg"))
}
pub fn render(package:&Path,snapshot:&Path,output:&Path,mut cancelled:impl FnMut()->bool,
 mut progress:impl FnMut(usize,usize))->Result<Receipt,Error>{
 if !package.is_absolute()||!snapshot.is_absolute()||!output.is_absolute(){return Err(Error::Invalid);}
 let package=package.canonicalize()?;
 if snapshot.starts_with(&package)||output.starts_with(&package)||snapshot==output{return Err(Error::Invalid);}
 let m:Manifest=serde_json::from_str(&read_text(&package.join("manifest.json"),1024*1024)?).map_err(|_|Error::Invalid)?;
 let n=u64::from(m.width)*u64::from(m.height);
 // Initial preview lane restricted to WGS84 UTM north/south metric coordinates.
 if m.version!=1||!((32601..=32660).contains(&m.epsg)||(32701..=32760).contains(&m.epsg))||
 m.width==0||m.height==0||n>64_000_000||!m.gsd.is_finite()||m.gsd<=0.||
 !m.origin_x.is_finite()||!m.origin_y.is_finite()||m.files.len()<5||m.files.len()>259||
 m.files.keys().any(|k|!allowed(k)){return Err(Error::Invalid);}
 for name in ["cameras.txt","images.txt","dem.raw"]{if !m.files.contains_key(name){return Err(Error::Invalid);}}
 if !fs::symlink_metadata(package.join("images"))?.file_type().is_dir(){return Err(Error::Invalid);}
 fs::create_dir(snapshot)?;fs::create_dir(snapshot.join("images"))?;
 let mut copied=0u64;
 for(name,hash)in &m.files{
 let cap=match name.as_str(){"dem.raw"=>n*4,"cameras.txt"=>16*1024*1024,"images.txt"=>128*1024*1024,_=>128*1024*1024};
 let bytes=copy_verified(&package.join(name),&snapshot.join(name),hash,cap.min(MAX_INPUT-copied),&mut cancelled)?;
 copied+=bytes;if name=="dem.raw"&&bytes!=n*4{return Err(Error::Invalid);}
 }
 let views=colmap_model::parse(&read_text(&snapshot.join("cameras.txt"),16*1024*1024)?,
 &read_text(&snapshot.join("images.txt"),128*1024*1024)?,256).map_err(|_|Error::Invalid)?;
 if views.len()<2||views.len()+3!=m.files.len()||views.iter().any(|v|!m.files.contains_key(&format!("images/{}",v.name))){return Err(Error::Invalid);}
 let align=colmap_model::Alignment{scale:m.scale,rotation:m.rotation,offset:m.offset};
 let mut src=jpeg_source::Source::from_model(&snapshot.join("images"),snapshot.join("dem.raw"),m.width,m.height,&views,align,&BTreeMap::new(),
 jpeg_source::Policy{max_compressed_bytes:128*1024*1024,max_decode_bytes:256*1024*1024,cache_bytes:128*1024*1024,
 header:images::Limits{max_header_bytes:1024*1024,max_side:16384,max_pixels:32_000_000}}).map_err(Error::Render)?;
 fs::create_dir(output)?;let store=disk_store::Store::open(output).map_err(|e|Error::Render(pipeline::Error::Store(e)))?;
 let g=ortho::Grid{origin_x:m.origin_x,origin_y:m.origin_y,gsd:m.gsd,width:m.width,height:m.height};
 let p=pipeline::Policy{tiles:tiles::Policy{min_side:32,max_side:512,halo_px:0,bytes_per_pixel:64,max_working_bytes:64*1024*1024,max_tiles:8192,max_depth:20},
 source_bytes:512*1024*1024,tile_bytes:8*1024*1024,max_samples:100_000_000,output_bytes:300*1024*1024};
 let q=pipeline::run_controlled(&mut src,&store,0,"preview",g,m.epsg,p,&mut cancelled,&mut progress).map_err(|e|if matches!(e,pipeline::Error::Cancelled){Error::Cancelled}else{Error::Render(e)})?;
 Ok(Receipt{revision:q.revision,tiles:q.tiles,pixels:q.pixels,covered:q.covered,written_bytes:q.written_bytes,epsg:m.epsg})
}
#[cfg(test)]mod tests{
 use super::*;
 #[test]fn fixed_package_paths_only(){assert!(allowed("images/a.jpg"));assert!(!allowed("images/../a.jpg"));assert!(!allowed("other.raw"));assert!(!allowed("images/a.png"));}
 #[test]fn rejects_wrong_hash(){
 let dir=std::env::temp_dir().join(format!("ovg-preview-{}",uuid::Uuid::new_v4()));fs::create_dir(&dir).unwrap();fs::write(dir.join("a"),b"abc").unwrap();
 assert!(matches!(copy_verified(&dir.join("a"),&dir.join("b"),&"0".repeat(64),10,&mut||false),Err(Error::Invalid)));let _=fs::remove_dir_all(dir);
 }
}
