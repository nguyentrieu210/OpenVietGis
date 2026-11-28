//! Non-authoritative presentation metadata. Never stores feature edits or source credentials.
use serde::{Serialize,Deserialize};
use std::{collections::HashSet,fs,io::{Read,Write},path::Path};
use uuid::Uuid;
type Result<T>=std::result::Result<T,&'static str>;
const MAX:usize=64*1024;
#[derive(Clone,Debug,Serialize,Deserialize,PartialEq)]
#[serde(deny_unknown_fields)]
pub struct View{pub lon:f64,pub lat:f64,pub zoom:f64,pub bearing:f64,pub pitch:f64}
impl Default for View{fn default()->Self{Self{lon:106.7,lat:11.,zoom:5.,bearing:0.,pitch:0.}}}
#[derive(Clone,Debug,Serialize,Deserialize,PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Style{pub color:String,pub opacity:f64}
impl Default for Style{fn default()->Self{Self{color:"#1976a3".into(),opacity:0.25}}}
#[derive(Clone,Debug,Serialize,Deserialize,PartialEq)]
#[serde(deny_unknown_fields)]
pub struct LayerView{pub id:String,pub visible:bool,pub style:Style}
#[derive(Clone,Debug,Serialize,Deserialize,PartialEq)]
#[serde(deny_unknown_fields)]
pub struct ProjectState{pub version:u32,pub revision:u64,pub view:View,pub active:String,pub layers:Vec<LayerView>}
impl ProjectState{
 pub fn initial()->Self{Self{version:1,revision:0,view:View::default(),active:"root".into(),layers:vec![LayerView{id:"root".into(),visible:true,style:Style::default()}]}}
 pub fn validate(&self,known:&HashSet<String>)->Result<()>{
  if self.version!=1||self.revision>9_007_199_254_740_991||self.layers.is_empty()||self.layers.len()>33{return Err("INVALID_PROJECT_STATE")}
  let v=&self.view;
  if [v.lon,v.lat,v.zoom,v.bearing,v.pitch].iter().any(|n|!n.is_finite())||v.lon.abs()>180.||v.lat.abs()>85.051129||!(0.0..=24.).contains(&v.zoom)||v.bearing.abs()>360.||!(0.0..=85.).contains(&v.pitch){return Err("INVALID_VIEW")}
  let mut seen=HashSet::new();
  for l in &self.layers{
    if !known.contains(&l.id)||!seen.insert(l.id.as_str())||l.style.color.len()!=7||!l.style.color.starts_with('#')||!l.style.color.as_bytes()[1..].iter().all(u8::is_ascii_hexdigit)||!l.style.opacity.is_finite()||!(0.0..=1.).contains(&l.style.opacity){return Err("INVALID_LAYER_VIEW")}
  }
  if !seen.contains("root")||!self.layers.iter().any(|l|l.id==self.active&&l.visible){return Err("INVALID_ACTIVE_LAYER")}Ok(())
 }
 pub fn reconcile(&mut self,ids:&[Uuid]){
   let known=known(ids);
   self.layers.retain(|l|known.contains(&l.id));
   for id in std::iter::once("root".to_owned()).chain(ids.iter().map(Uuid::to_string)){
     if !self.layers.iter().any(|l|l.id==id){self.layers.push(LayerView{id,visible:false,style:Style::default()});}
   }
   if !self.layers.iter().any(|l|l.id==self.active&&l.visible){self.active="root".into();if let Some(l)=self.layers.iter_mut().find(|l|l.id=="root"){l.visible=true}}
 }
}
pub fn known(ids:&[Uuid])->HashSet<String>{std::iter::once("root".to_owned()).chain(ids.iter().map(Uuid::to_string)).collect()}
pub fn read(dir:&Path,ids:&[Uuid])->Result<ProjectState>{
 let p=dir.join("project-state.json");
 let mut s=match fs::symlink_metadata(&p){
  Err(e) if e.kind()==std::io::ErrorKind::NotFound=>ProjectState::initial(),
  Ok(m) if m.file_type().is_file()&&m.len()<=MAX as u64=>{
   let mut b=Vec::new();fs::File::open(&p).map_err(|_|"STORAGE_UNAVAILABLE")?.take(MAX as u64+1).read_to_end(&mut b).map_err(|_|"INVALID_PROJECT_STATE")?;
   if b.len()>MAX{return Err("RESOURCE_LIMIT")}
   serde_json::from_slice(&b).map_err(|_|"INVALID_PROJECT_STATE")?
  },_=>return Err("INVALID_PROJECT_STATE")
 };
 s.reconcile(ids);s.validate(&known(ids))?;Ok(s)
}
pub fn save(dir:&Path,ids:&[Uuid],mut next:ProjectState,expected:u64)->Result<ProjectState>{
 let current=read(dir,ids)?;
 if current.revision!=expected||next.revision!=expected{return Err("REVISION_CONFLICT")}
 next.revision=expected.checked_add(1).ok_or("RESOURCE_LIMIT")?;
 next.validate(&known(ids))?;write_exact(dir,&next)?;Ok(next)
}
pub fn write_exact(dir:&Path,s:&ProjectState)->Result<()>{
 let data=serde_json::to_vec(s).map_err(|_|"INVALID_PROJECT_STATE")?;
 if data.len()>MAX{return Err("RESOURCE_LIMIT")}
 let mut tmp=tempfile::NamedTempFile::new_in(dir).map_err(|_|"STORAGE_UNAVAILABLE")?;
 tmp.write_all(&data).map_err(|_|"STORAGE_UNAVAILABLE")?;
 tmp.as_file().sync_all().map_err(|_|"STORAGE_UNAVAILABLE")?;
 tmp.persist(dir.join("project-state.json")).map_err(|_|"STORAGE_UNAVAILABLE")?;
 Ok(())
}
#[cfg(test)]mod tests{
 use super::*;
 #[test]fn persists_and_rejects_stale_state(){
  let d=tempfile::tempdir().unwrap();let id=Uuid::new_v4();
  let mut s=read(d.path(),&[id]).unwrap();s.view.zoom=13.;
  s.layers[1].visible=true;s.active=id.to_string();
  let saved=save(d.path(),&[id],s.clone(),0).unwrap();assert_eq!(saved.revision,1);
  assert_eq!(read(d.path(),&[id]).unwrap(),saved);
  assert!(save(d.path(),&[id],s,0).is_err());
 }
 #[test]fn rejects_unknown_layer_and_invalid_style(){
  let d=tempfile::tempdir().unwrap();let mut s=ProjectState::initial();
  s.layers[0].style.color="url(https://example.org)".into();assert!(save(d.path(),&[],s,0).is_err());
 }
}
