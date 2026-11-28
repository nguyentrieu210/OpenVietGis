//! Stage original JPEGs without decoding whole images; trusted file-picker paths only.
//! Fresh isolated staging directory, immutable source copy and SHA256 inventory.
use super::images::{self,Dimensions,Limits};
use sha2::{Digest,Sha256};
use std::{fs::{self,File,OpenOptions},io::{Read,Write},path::{Path,PathBuf}};
#[derive(Debug)]pub enum Error{Invalid,Limit,Io(std::io::Error),Header(images::Error),Changed}
impl From<std::io::Error> for Error{fn from(e:std::io::Error)->Self{Self::Io(e)}}
#[derive(Debug,Clone)]pub struct Entry{pub name:String,pub source:PathBuf,pub sha256:String,pub bytes:u64,pub dimensions:Dimensions}
#[derive(Debug,Clone,Copy)]pub struct Policy{pub max_files:usize,pub max_file_bytes:u64,pub max_total_bytes:u64,pub header:Limits}
pub fn stage_jpeg(root:&Path,paths:&[PathBuf],p:Policy)->Result<Vec<Entry>,Error>{
 if !root.is_absolute()||paths.len()<2||paths.len()>p.max_files||p.max_files>100_000||
 p.max_file_bytes==0||p.max_file_bytes>1024*1024*1024||p.max_total_bytes==0||p.header.max_header_bytes==0||p.header.max_header_bytes>4*1024*1024{
 return Err(Error::Invalid);}
 let mut entries=Vec::with_capacity(paths.len());let mut total=0u64;let mut seen=std::collections::BTreeSet::new();
 // Preflight before creating a new job; partial stage never becomes a usable job.
 for path in paths{
 let m=fs::symlink_metadata(path)?;if !m.is_file()||m.file_type().is_symlink(){return Err(Error::Invalid);}
 let path=path.canonicalize()?;if !seen.insert(path.clone()){return Err(Error::Invalid);}
 if m.len()==0||m.len()>p.max_file_bytes{return Err(Error::Limit);}
 total=total.checked_add(m.len()).ok_or(Error::Limit)?;if total>p.max_total_bytes{return Err(Error::Limit);}
 let mut head=Vec::new();File::open(&path)?.take(p.header.max_header_bytes as u64).read_to_end(&mut head)?;
 let dimensions=images::jpeg_dimensions(&head,p.header).map_err(Error::Header)?;
 entries.push(Entry{name:format!("image_{:06}.jpg",entries.len()+1),source:path,sha256:String::new(),bytes:m.len(),dimensions});
 }
 fs::create_dir(root)?;fs::create_dir(root.join("images"))?;fs::create_dir(root.join("sparse"))?;
 for e in &mut entries{
 let mut src=File::open(&e.source)?;
 let mut dst=OpenOptions::new().create_new(true).write(true).open(root.join("images").join(&e.name))?;
 let mut h=Sha256::new();let mut n=0u64;let mut b=[0u8;64*1024];
 loop{let k=src.read(&mut b)?;if k==0{break;}n=n.checked_add(k as u64).ok_or(Error::Limit)?;
 if n>e.bytes{return Err(Error::Changed);}h.update(&b[..k]);dst.write_all(&b[..k])?;}
 if n!=e.bytes{return Err(Error::Changed);}dst.sync_all()?;e.sha256=format!("{:x}",h.finalize());
 let mut header=Vec::new();File::open(root.join("images").join(&e.name))?.take(p.header.max_header_bytes as u64).read_to_end(&mut header)?;
 if images::jpeg_dimensions(&header,p.header).map_err(Error::Header)?!=e.dimensions{return Err(Error::Changed);}
 }
 let manifest:Vec<StageRecord>=entries.iter().map(|e|StageRecord{name:e.name.clone(),sha256:e.sha256.clone(),bytes:e.bytes,width:e.dimensions.width,height:e.dimensions.height}).collect();
 let json=serde_json::to_vec(&manifest).map_err(|_|Error::Invalid)?;
 if json.len()>32*1024*1024{return Err(Error::Limit);}
 let mut f=OpenOptions::new().write(true).create_new(true).open(root.join("input-manifest.json"))?;
 f.write_all(&json)?;f.sync_all()?;
 Ok(entries)
}

#[derive(serde::Serialize,serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct StageRecord{name:String,sha256:String,bytes:u64,width:u32,height:u32}

/// Verify a completed manifest before admitting a worker. Hashes establish consistency,
/// not authorization; the trusted host must bind the manifest digest to its job record.
pub fn verify_staged(root:&Path,expected_count:u32,max_total_bytes:u64,mut cancelled:impl FnMut()->bool)->Result<String,Error>{
 if expected_count<2||expected_count>100_000||max_total_bytes==0{return Err(Error::Invalid);}
 let p=root.join("input-manifest.json");let meta=fs::symlink_metadata(&p)?;
 if !meta.is_file()||meta.file_type().is_symlink()||meta.len()>32*1024*1024{return Err(Error::Invalid);}
 let mut data=Vec::new();File::open(p)?.take(32*1024*1024+1).read_to_end(&mut data)?;
 if data.len()>32*1024*1024{return Err(Error::Limit);}
 let rows:Vec<StageRecord>=serde_json::from_slice(&data).map_err(|_|Error::Invalid)?;
 if rows.len()!=expected_count as usize{return Err(Error::Invalid);}
 let mut total=0u64;
 for (i,r) in rows.iter().enumerate(){
 if cancelled(){return Err(Error::Changed);}
 if r.name!=format!("image_{:06}.jpg",i+1)||!super::digest(&r.sha256)||r.bytes==0||r.bytes>1024*1024*1024||
 r.width==0||r.height==0{return Err(Error::Invalid);}
 total=total.checked_add(r.bytes).ok_or(Error::Limit)?;
 if total>max_total_bytes{return Err(Error::Limit);}
 let p=root.join("images").join(&r.name);let m=fs::symlink_metadata(&p)?;
 if !m.is_file()||m.file_type().is_symlink()||m.len()!=r.bytes{return Err(Error::Changed);}
 let mut f=File::open(p)?;let mut h=Sha256::new();let mut buf=[0;64*1024];let mut n=0u64;
 loop{if cancelled(){return Err(Error::Changed);}let k=f.read(&mut buf)?;if k==0{break;}
 n+=k as u64;if n>r.bytes{return Err(Error::Changed);}h.update(&buf[..k]);}
 if n!=r.bytes||format!("{:x}",h.finalize())!=r.sha256{return Err(Error::Changed);}
 }
 Ok(format!("{:x}",Sha256::digest(&data)))
}
#[cfg(test)]mod tests{
 use super::*;use uuid::Uuid;
 struct Temp(PathBuf);impl Drop for Temp{fn drop(&mut self){let _=fs::remove_dir_all(&self.0);}}
 fn fixture()->(Temp,Vec<PathBuf>){
 let t=Temp(std::env::temp_dir().join(format!("ovg-ingest-{}",Uuid::new_v4())));fs::create_dir(&t.0).unwrap();
 let paths=vec![t.0.join("a.jpg"),t.0.join("b.jpg")];
 let bytes=[0xffu8,0xd8,0xff,0xc0,0,11,8,0,7,0,9,1,1,0x11,0];
 for p in &paths{fs::write(p,bytes).unwrap();}(t,paths)}
 fn policy()->Policy{Policy{max_files:5,max_file_bytes:100,max_total_bytes:200,header:Limits{max_header_bytes:100,max_side:32,max_pixels:1024}}}
 #[test]fn staged_names_hash_and_source_unchanged(){
 let(t,paths)=fixture();let root=t.0.join("stage");let e=stage_jpeg(&root,&paths,policy()).unwrap();
 assert_eq!(e[0].name,"image_000001.jpg");assert_eq!(e[0].dimensions,Dimensions{width:9,height:7});
 assert_eq!(e[0].sha256,e[1].sha256);assert_eq!(fs::read(&paths[0]).unwrap(),fs::read(root.join("images/image_000001.jpg")).unwrap());
 assert!(stage_jpeg(&root,&paths,policy()).is_err());}
 #[test]fn verifies_manifest_and_detects_same_size_tampering(){
 let(t,paths)=fixture();let root=t.0.join("stage");stage_jpeg(&root,&paths,policy()).unwrap();
 let hash=verify_staged(&root,2,200,||false).unwrap();assert!(super::super::digest(&hash));
 let file=root.join("images/image_000001.jpg");let mut bytes=fs::read(&file).unwrap();bytes[6]^=1;fs::write(file,bytes).unwrap();
 assert!(matches!(verify_staged(&root,2,200,||false),Err(Error::Changed)));}
 #[test]fn duplicate_and_total_cap(){
 let(t,paths)=fixture();let mut p=policy();p.max_total_bytes=10;
 assert!(matches!(stage_jpeg(&t.0.join("a"),&paths,p),Err(Error::Limit)));
 assert!(matches!(stage_jpeg(&t.0.join("b"),&[paths[0].clone(),paths[0].clone()],policy()),Err(Error::Invalid)));}
}
