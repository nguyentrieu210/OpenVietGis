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
 p.max_file_bytes==0||p.max_total_bytes==0||p.header.max_header_bytes==0||p.header.max_header_bytes>4*1024*1024{
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
 }
 Ok(entries)
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
 #[test]fn duplicate_and_total_cap(){
 let(t,paths)=fixture();let mut p=policy();p.max_total_bytes=10;
 assert!(matches!(stage_jpeg(&t.0.join("a"),&paths,p),Err(Error::Limit)));
 assert!(matches!(stage_jpeg(&t.0.join("b"),&[paths[0].clone(),paths[0].clone()],policy()),Err(Error::Invalid)));}
}
