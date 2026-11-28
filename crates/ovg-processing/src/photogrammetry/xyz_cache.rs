//! Bounded two-tier display cache for immutable preview revisions.
//! Persistent slots are disposable derived data, never source imagery.
use super::{disk_store,xyz};
use std::{collections::{BTreeMap,VecDeque},fs::{self,File,OpenOptions},io::{Read,Write},path::{Path,PathBuf},sync::{Arc,Mutex}};
const BYTES:usize=256*256*4;
const SLOTS:u16=256;
const MAGIC:&[u8;8]=b"OVGXYZ01";
const RECORD:usize=8+64+64+BYTES;
const VERSION:&str="xyz-nearest-utm-v1";
#[derive(Debug,Clone,Copy,PartialEq,Eq)]pub enum Hit{Memory,Disk,Rendered}
#[derive(Debug,Clone,PartialEq,Eq,PartialOrd,Ord)]struct Key{root:PathBuf,digest:String}
struct Memory{items:BTreeMap<Key,Arc<[u8]>>,order:VecDeque<Key>,capacity:usize}
impl Memory{
 fn get(&mut self,k:&Key)->Option<Arc<[u8]>>{let a=self.items.get(k)?.clone();self.order.retain(|v|v!=k);self.order.push_back(k.clone());Some(a)}
 fn put(&mut self,k:Key,v:Arc<[u8]>){
 if self.capacity==0{return;}
 self.order.retain(|a|a!=&k);self.items.insert(k.clone(),v);self.order.push_back(k);
 while self.items.len()>self.capacity{if let Some(a)=self.order.pop_front(){self.items.remove(&a);}else{break;}}
 }
}
pub struct Cache{memory:Mutex<Memory>,disk:Mutex<()>}
impl Cache{
 /// Up to 256 RAM tiles (64 MiB). In-flight return values are charged separately.
 pub fn new(ram_bytes:usize)->Self{Self{memory:Mutex::new(Memory{items:BTreeMap::new(),order:VecDeque::new(),capacity:(ram_bytes/BYTES).min(256)}),disk:Mutex::new(())}}
 /// Revision identity covers manifest contents, exact source references and render semantics.
 fn key(root:&Path,z:u8,x:u32,y:u32)->Result<Key,xyz::Error>{
 if z>24||x>=1u32<<z||y>=1u32<<z{return Err(xyz::Error::Invalid);}
 let root=root.canonicalize().map_err(|e|xyz::Error::Store(e.into()))?;
 let path=root.join("revisions/00000000000000000001.json");
 let meta=fs::symlink_metadata(&path).map_err(|e|xyz::Error::Store(e.into()))?;
 if !meta.file_type().is_file()||meta.len()>4*1024*1024{return Err(xyz::Error::Invalid);}
 let manifest=disk_store::bounded_read(&path,4*1024*1024).map_err(xyz::Error::Store)?;
 let mut input=format!("{VERSION}:{z}:{x}:{y}:").into_bytes();input.extend(manifest);
 Ok(Key{root,digest:disk_store::hash(&input)})
 }
 fn directory(k:&Key)->Option<PathBuf>{
 let p=k.root.join(".xyz-cache-v1");
 if !p.exists(){let _=fs::create_dir(&p);}
 if !fs::symlink_metadata(&p).ok()?.file_type().is_dir(){return None;}Some(p)
 }
 fn slot(k:&Key)->String{let n=u16::from_str_radix(&k.digest[..4],16).unwrap_or(0)%SLOTS;format!("tile-{n:03}.bin")}
 fn load(k:&Key)->Option<Arc<[u8]>>{
 let p=Self::directory(k)?.join(Self::slot(k));
 let m=fs::symlink_metadata(&p).ok()?;if !m.file_type().is_file()||m.len()!=RECORD as u64{return None;}
 let mut b=Vec::with_capacity(RECORD);File::open(p).ok()?.take(RECORD as u64+1).read_to_end(&mut b).ok()?;
 if b.len()!=RECORD||&b[..8]!=MAGIC||&b[8..72]!=k.digest.as_bytes()||disk_store::hash(&b[136..]).as_bytes()!=&b[72..136]{return None;}
 Some(Arc::from(&b[136..]))
 }
 fn save(k:&Key,b:&[u8])->Option<()>{
 if b.len()!=BYTES{return None;}
 let dir=Self::directory(k)?;let path=dir.join(Self::slot(k));let temp=dir.join(".pending-tile");
 // Single process writer; do not overwrite unknown files or follow symlinks.
 // A leftover crash temp disables disk writes until explicitly cleaned.
 let mut f=OpenOptions::new().write(true).create_new(true).open(&temp).ok()?;
 let result=(||{
 f.write_all(MAGIC).ok()?;f.write_all(k.digest.as_bytes()).ok()?;
 f.write_all(disk_store::hash(b).as_bytes()).ok()?;f.write_all(b).ok()?;f.sync_all().ok()?;drop(f);
 if let Ok(m)=fs::symlink_metadata(&path){
 if !m.file_type().is_file()||m.len()!=RECORD as u64{return None;}
 // Windows rename does not reliably replace an existing file. Under the
 // process-local disk lock, dropping an old cache slot is safe.
 fs::remove_file(&path).ok()?;
 }
 fs::rename(&temp,&path).ok()?;Some(())
 })();
 let _=fs::remove_file(temp);result
 }
 /// Cache corruption/miss falls back to verified source rendering.
 /// Host must enforce immutable source directory and project access before calling.
 pub fn get(&self,root:&Path,z:u8,x:u32,y:u32,mut cancelled:impl FnMut()->bool)->Result<(Arc<[u8]>,Hit),xyz::Error>{
 if cancelled(){return Err(xyz::Error::Cancelled);}
 let k=Self::key(root,z,x,y)?;
 if let Some(v)=self.memory.lock().ok().and_then(|mut m|m.get(&k)){
 if cancelled(){return Err(xyz::Error::Cancelled);}return Ok((v,Hit::Memory));}
 let disk=self.disk.lock().ok().and_then(|_guard|Self::load(&k));
 if let Some(v)=disk{if cancelled(){return Err(xyz::Error::Cancelled);}
 if let Ok(mut m)=self.memory.lock(){m.put(k,v.clone());}return Ok((v,Hit::Disk));}
 let b:Arc<[u8]>=Arc::from(xyz::render(&k.root,z,x,y,&mut cancelled)?);
 if cancelled(){return Err(xyz::Error::Cancelled);}
 if Self::key(&k.root,z,x,y)?!=k{return Err(xyz::Error::Invalid);}
 if let Ok(_guard)=self.disk.lock(){let _=Self::save(&k,&b);}
 if let Ok(mut m)=self.memory.lock(){m.put(k,b.clone());}
 if cancelled(){return Err(xyz::Error::Cancelled);}Ok((b,Hit::Rendered))
 }
}
#[cfg(test)]mod tests{
 use super::*;
 #[test]fn ram_lru_eviction(){
 let mut m=Memory{items:BTreeMap::new(),order:VecDeque::new(),capacity:2};
 let key=|n:u8|Key{root:PathBuf::from("/test"),digest:n.to_string()};
 let b:Arc<[u8]>=Arc::from(vec![0u8;BYTES]);
 m.put(key(1),b.clone());m.put(key(2),b.clone());assert!(m.get(&key(1)).is_some());
 m.put(key(3),b);assert!(m.get(&key(2)).is_none());assert!(m.get(&key(1)).is_some());
 }
 #[test]fn disk_roundtrip_and_corruption(){
 let root=std::env::temp_dir().join(format!("ovg-cache-{}",uuid::Uuid::new_v4()));fs::create_dir(&root).unwrap();
 let k=Key{root:root.clone(),digest:"a".repeat(64)};let b=vec![23;BYTES];Cache::save(&k,&b).unwrap();
 assert_eq!(&*Cache::load(&k).unwrap(),&b[..]);
 let k2=Key{root:root.clone(),digest:format!("{}b","a".repeat(63))};assert_eq!(Cache::slot(&k),Cache::slot(&k2));assert!(Cache::load(&k2).is_none());
 fs::write(Cache::directory(&k).unwrap().join(Cache::slot(&k)),vec![0;RECORD]).unwrap();assert!(Cache::load(&k).is_none());
 let _=fs::remove_dir_all(root);
 }
}
