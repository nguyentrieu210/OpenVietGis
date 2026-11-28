//! Per-tile immutable restart journal. No completed revision is exposed until publish.
//! Input digest must bind actual immutable bytes and the qualified renderer build.
use super::{disk_store::{self,Store,Error,TileRef},ortho,tiles,pipeline};
use serde::{Deserialize,Serialize};
use std::{fs,path::PathBuf};

#[derive(Debug,Clone,PartialEq,Eq,Serialize,Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Record{pub tile:TileRef,pub covered:u64}
pub struct Checkpoint{dir:PathBuf}
impl Checkpoint{
 pub fn open(store:&Store,expected:u64,grid_id:&str,g:ortho::Grid,epsg:u16,p:pipeline::Policy,input:&str)->Result<Self,Error>{
  if !super::digest(input)||!super::valid_id(grid_id)||!g.gsd.is_finite()||g.gsd<=0.||
   !g.origin_x.is_finite()||!g.origin_y.is_finite()||g.width==0||g.height==0{return Err(Error::Invalid);}
  // Exact bit representations prevent coordinate rounding collisions.
  let key=serde_json::json!({
   "schema":"ovg-ortho-checkpoint-v1","input":input,"expected":expected,"grid":grid_id,"epsg":epsg,
   "origin_x":g.origin_x.to_bits(),"origin_y":g.origin_y.to_bits(),"gsd":g.gsd.to_bits(),
   "width":g.width,"height":g.height,"min_side":p.tiles.min_side,"max_side":p.tiles.max_side,
   "halo":p.tiles.halo_px,"bytes_per_pixel":p.tiles.bytes_per_pixel,"working":p.tiles.max_working_bytes,
   "max_tiles":p.tiles.max_tiles,"max_depth":p.tiles.max_depth,"source_bytes":p.source_bytes,
   "tile_bytes":p.tile_bytes,"max_samples":p.max_samples,"output_bytes":p.output_bytes
  });
  let parent=store.checkpoint_root();let dir=parent.join(disk_store::hash(&serde_json::to_vec(&key)?));
  fs::create_dir_all(&dir)?;let m=fs::symlink_metadata(&dir)?;
  if !m.is_dir()||m.file_type().is_symlink(){return Err(Error::Invalid);}
  disk_store::sync_dir(&parent)?;Ok(Self{dir})
 }
 fn check(store:&Store,r:&Record,window:tiles::Window,bytes:u64)->Result<(),Error>{
  if window.width==0||window.height==0||r.tile.window!=window||r.tile.bytes!=bytes||
   r.covered>u64::from(window.width)*u64::from(window.height){return Err(Error::Corrupt);}
  store.verify(&r.tile.sha256,r.tile.bytes)
 }
 pub fn load(&self,store:&Store,id:&str,window:tiles::Window,bytes:u64)->Result<Option<Record>,Error>{
  if !super::valid_id(id){return Err(Error::Invalid);}
  let path=self.dir.join(format!("{id}.json"));
  match fs::symlink_metadata(&path){
   Err(e)if e.kind()==std::io::ErrorKind::NotFound=>return Ok(None),
   Err(e)=>return Err(e.into()),Ok(m)if !m.is_file()||m.file_type().is_symlink()=>return Err(Error::Corrupt),_=>{}
  }
  let r:Record=serde_json::from_slice(&disk_store::bounded_read(&path,4096)?)?;
  Self::check(store,&r,window,bytes)?;Ok(Some(r))
 }
 pub fn save(&self,store:&Store,id:&str,r:Record)->Result<(),Error>{
  if !super::valid_id(id){return Err(Error::Invalid);}
  Self::check(store,&r,r.tile.window,r.tile.bytes)?;
  let data=serde_json::to_vec(&r)?;
  if !disk_store::publish_new(&self.dir,&format!("{id}.json"),&data)?{
   let old=self.load(store,id,r.tile.window,r.tile.bytes)?.ok_or(Error::Corrupt)?;
   if old!=r{return Err(Error::Conflict);}
  }
  Ok(())
 }
}
#[cfg(test)]mod tests{
 use super::*;
 #[test]fn checkpoint_is_scoped_and_validates_objects(){
 let dir=std::env::temp_dir().join(format!("ovg-checkpoint-{}",uuid::Uuid::new_v4()));
 let s=Store::open(&dir).unwrap();let g=ortho::Grid{origin_x:0.,origin_y:0.,gsd:1.,width:1,height:1};
 let p=pipeline::Policy{tiles:tiles::Policy{min_side:1,max_side:1,halo_px:0,bytes_per_pixel:64,max_working_bytes:1024,max_tiles:2,max_depth:2},source_bytes:1024,tile_bytes:1024,max_samples:100,output_bytes:1024};
 let c=Checkpoint::open(&s,0,"grid",g,32648,p,&"a".repeat(64)).unwrap();
 let (sha256,bytes)=s.put(b"test").unwrap();let window=tiles::Window{x:0,y:0,width:1,height:1};
 let record=Record{tile:TileRef{window,sha256:sha256.clone(),bytes},covered:1};
 c.save(&s,"tile",record.clone()).unwrap();assert_eq!(c.load(&s,"tile",window,bytes).unwrap(),Some(record));
 let other=Checkpoint::open(&s,0,"grid",g,32648,p,&"b".repeat(64)).unwrap();
 assert!(other.load(&s,"tile",window,bytes).unwrap().is_none());
 fs::write(dir.join("objects").join(sha256),b"bad!").unwrap();
 assert!(matches!(c.load(&s,"tile",window,bytes),Err(Error::Corrupt)));
 let _=fs::remove_dir_all(dir);
 }
}
