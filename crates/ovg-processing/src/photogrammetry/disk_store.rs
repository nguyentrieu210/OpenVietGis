//! Immutable file-backed artifacts and atomic revision publication by hard link.
//! Trusted local root only; not an IPC path resolver. No garbage collection.
//! Unix directory fsync is performed; power-loss durability on other OSes is unqualified.
use serde::{Deserialize,Serialize};
use sha2::{Digest,Sha256};
use std::{collections::BTreeMap,fs::{self,File,OpenOptions},io::{Read,Write},path::{Path,PathBuf}};
use uuid::Uuid;
use super::tiles::Window;
const MAX_MANIFEST:u64=32*1024*1024;
const MAX_ARTIFACT:u64=256*1024*1024;
#[derive(Debug)]pub enum Error{Io(std::io::Error),Json(serde_json::Error),Invalid,Limit,Conflict,Corrupt}
impl From<std::io::Error> for Error{fn from(e:std::io::Error)->Self{Self::Io(e)}}
impl From<serde_json::Error> for Error{fn from(e:serde_json::Error)->Self{Self::Json(e)}}
#[derive(Debug,Clone,Serialize,Deserialize,PartialEq,Eq)]
#[serde(deny_unknown_fields)]
pub struct TileRef{pub window:Window,pub sha256:String,pub bytes:u64}
#[derive(Debug,Clone,Serialize,Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Revision{pub id:u64,pub parent:u64,pub grid_id:String,pub epsg:u16,pub tiles:BTreeMap<String,TileRef>}
pub struct Store{root:PathBuf}
pub(super) fn hash(bytes:&[u8])->String{format!("{:x}",Sha256::digest(bytes))}
pub(super) fn sync_dir(path:&Path)->Result<(),Error>{
 #[cfg(unix)]{File::open(path)?.sync_all()?;}
 #[cfg(not(unix))]{let _=path;}
 Ok(())
}
pub(super) fn bounded_read(path:&Path,max:u64)->Result<Vec<u8>,Error>{
 let f=File::open(path)?;if !f.metadata()?.is_file()||f.metadata()?.len()>max{return Err(Error::Limit);}
 let mut data=Vec::new();f.take(max+1).read_to_end(&mut data)?;
 if data.len() as u64>max{return Err(Error::Limit);}Ok(data)
}
/// Write a fresh temp file, fsync it, then publish without overwriting a target.
/// Hard-link requires same-volume filesystem support; otherwise fail explicitly.
pub(super) fn publish_new(dir:&Path,name:&str,data:&[u8])->Result<bool,Error>{
 let temp=dir.join(format!(".pending-{}",Uuid::new_v4()));let target=dir.join(name);
 let result=(||{
    let mut f=OpenOptions::new().write(true).create_new(true).open(&temp)?;
    f.write_all(data)?;f.sync_all()?;drop(f);
    match fs::hard_link(&temp,&target){
      Ok(())=>{sync_dir(dir)?;Ok(true)},
      Err(e)if e.kind()==std::io::ErrorKind::AlreadyExists=>Ok(false),
      Err(e)=>Err(Error::Io(e)),
    }
 })();
 let _=fs::remove_file(&temp);result
}
impl Store{
 pub fn open(root:&Path)->Result<Self,Error>{
  if !root.is_absolute(){return Err(Error::Invalid);}
  fs::create_dir_all(root)?;let root=root.canonicalize()?;
  for n in ["objects","revisions","checkpoints"]{let p=root.join(n);fs::create_dir_all(&p)?;
   let m=fs::symlink_metadata(&p)?;if !m.is_dir()||m.file_type().is_symlink(){return Err(Error::Invalid);}}
  sync_dir(&root)?;Ok(Self{root})
 }
 pub(super) fn checkpoint_root(&self)->PathBuf{self.root.join("checkpoints")}
 pub fn put(&self,bytes:&[u8])->Result<(String,u64),Error>{
  if bytes.is_empty()||bytes.len() as u64>MAX_ARTIFACT{return Err(Error::Limit);}
  let key=hash(bytes);let dir=self.root.join("objects");
  if !publish_new(&dir,&key,bytes)?{self.verify(&key,bytes.len() as u64)?;}
  Ok((key,bytes.len() as u64))
 }
 pub fn verify(&self,key:&str,len:u64)->Result<(),Error>{
  if !super::digest(key)||len==0||len>MAX_ARTIFACT{return Err(Error::Invalid);}
  let p=self.root.join("objects").join(key);
  let mut f=File::open(p)?;if !f.metadata()?.is_file()||f.metadata()?.len()!=len{return Err(Error::Corrupt);}
  let mut h=Sha256::new();let mut b=[0u8;64*1024];let mut n=0u64;
  loop{let k=f.read(&mut b)?;if k==0{break;}n+=k as u64;if n>len{return Err(Error::Corrupt);}h.update(&b[..k]);}
  if n!=len||format!("{:x}",h.finalize())!=key{return Err(Error::Corrupt);}Ok(())
 }
 fn name(id:u64)->String{format!("{id:020}.json")}
 pub fn revision(&self,id:u64)->Result<Revision,Error>{
  if id==0{return Err(Error::Invalid);}
  let bytes=bounded_read(&self.root.join("revisions").join(Self::name(id)),MAX_MANIFEST)?;
  let r:Revision=serde_json::from_slice(&bytes)?;
  if r.id!=id||r.parent!=id-1||!super::valid_id(&r.grid_id)||r.epsg==0||r.epsg>=32767||
     r.tiles.is_empty()||r.tiles.len()>100_000{return Err(Error::Corrupt);}Ok(r)
 }
 pub fn head(&self)->Result<u64,Error>{
  let mut max=0;let mut count=0u64;
  for e in fs::read_dir(self.root.join("revisions"))?{
   let e=e?;let name=e.file_name();let n=name.to_str().ok_or(Error::Corrupt)?;
   if n.starts_with(".pending-"){continue;}
   let id=n.strip_suffix(".json").and_then(|s|s.parse::<u64>().ok()).filter(|v|*v>0&&Self::name(*v)==n).ok_or(Error::Corrupt)?;
   count+=1;if count>100_000{return Err(Error::Limit);}max=max.max(id);
  }
  if count!=max{return Err(Error::Corrupt);}
  if max>0{self.revision(max)?;}Ok(max)
 }
 pub fn publish(&self,expected:u64,grid_id:&str,epsg:u16,tiles:BTreeMap<String,TileRef>)->Result<u64,Error>{
  if !super::valid_id(grid_id)||epsg==0||epsg>=32767||tiles.is_empty()||tiles.len()>100_000{return Err(Error::Invalid);}
  if self.head()?!=expected{return Err(Error::Conflict);}
  if expected>0{let previous=self.revision(expected)?;if previous.grid_id!=grid_id||previous.epsg!=epsg{return Err(Error::Invalid);}}
  for (id,t) in &tiles{
   if !super::valid_id(id)||t.window.width==0||t.window.height==0||
      t.window.x.checked_add(t.window.width).is_none()||t.window.y.checked_add(t.window.height).is_none(){return Err(Error::Invalid);}
   self.verify(&t.sha256,t.bytes)?;
  }
  let id=expected.checked_add(1).ok_or(Error::Limit)?;if id>100_000{return Err(Error::Limit);}
  let r=Revision{id,parent:expected,grid_id:grid_id.into(),epsg,tiles};
  let bytes=serde_json::to_vec(&r)?;if bytes.len() as u64>MAX_MANIFEST{return Err(Error::Limit);}
  if !publish_new(&self.root.join("revisions"),&Self::name(id),&bytes)?{return Err(Error::Conflict);}
  Ok(id)
 }
}
#[cfg(test)]mod tests{
 use super::*;
 struct Temp(PathBuf);
 impl Drop for Temp{fn drop(&mut self){let _=fs::remove_dir_all(&self.0);}}
 fn temp()->Temp{Temp(std::env::temp_dir().join(format!("ovg-photo-{}",Uuid::new_v4())))}
 fn tile(s:&Store,b:&[u8])->TileRef{let(sha256,bytes)=s.put(b).unwrap();TileRef{window:Window{x:0,y:0,width:1,height:1},sha256,bytes}}
 #[test]fn persistent_reopen_and_cas(){
 let p=temp();let s=Store::open(&p.0).unwrap();let a=tile(&s,b"first");
 assert_eq!(s.publish(0,"grid",32648,[("t".into(),a.clone())].into_iter().collect()).unwrap(),1);
 let s=Store::open(&p.0).unwrap();assert_eq!(s.head().unwrap(),1);assert_eq!(s.revision(1).unwrap().tiles["t"],a);
 assert!(matches!(s.publish(0,"grid",32648,[("t".into(),a.clone())].into_iter().collect()),Err(Error::Conflict)));
 let b=tile(&s,b"second");s.publish(1,"grid",32648,[("t".into(),b)].into_iter().collect()).unwrap();
 assert_eq!(s.revision(1).unwrap().tiles["t"],a);}
 #[test]fn rejects_corrupted_artifact(){
 let p=temp();let s=Store::open(&p.0).unwrap();let a=tile(&s,b"first");
 fs::write(s.root.join("objects").join(&a.sha256),b"wrong").unwrap();
 assert!(matches!(s.publish(0,"grid",32648,[("t".into(),a)].into_iter().collect()),Err(Error::Corrupt)));
 assert_eq!(s.head().unwrap(),0);}
}
