//! Ephemeral viewport MVT archive for native renderers, not authoritative data.
//! No HTTP listener, no caller SQL, no arbitrary export destination.
use super::*;
use std::{io::Write,path::PathBuf,sync::atomic::{AtomicBool,Ordering}};
use flate2::{write::GzEncoder,Compression};
const MAX_TILES:usize=32;
const MAX_BYTES:usize=32*1024*1024;
#[derive(Clone,Debug,Serialize)]
pub struct ArchiveInfo{pub path:PathBuf,pub revision:String,pub zoom:u8,pub tiles:usize,pub bytes:usize,pub layers:Vec<Uuid>}
pub struct TileArchive{pub info:ArchiveInfo,_dir:tempfile::TempDir}
fn xyz(lon:f64,lat:f64,z:u8)->(u32,u32){
 let n=f64::from(1u32<<z);
 let lat=lat.clamp(-85.0511287798066,85.0511287798066).to_radians();
 let x=((lon+180.)/360.*n).floor().clamp(0.,n-1.);
 let y=((1.-lat.tan().asinh()/std::f64::consts::PI)/2.*n).floor().clamp(0.,n-1.);
 (x as u32,y as u32)
}
pub fn viewport_tiles(b:[f64;4],z:u8)->Result<Vec<(u32,u32)>>{
 if z>22||b.iter().any(|v|!v.is_finite())||b[0]< -180.||b[2]>180.||b[1]< -85.051129||b[3]>85.051129||b[0]>b[2]||b[1]>b[3]{return Err(Error::Invalid)}
 let (x0,y0)=xyz(b[0],b[3],z);let(x1,y1)=xyz(b[2],b[1],z);
 if u64::from(x1-x0+1)*u64::from(y1-y0+1)>MAX_TILES as u64{return Err(Error::Limit)}
 let mut out=Vec::new();for y in y0..=y1{for x in x0..=x1{out.push((x,y));}}Ok(out)
}
impl TileArchive{
 pub fn build(store:&Store,layers:&[Uuid],revision:u64,b:[f64;4],z:u8,root:&Path,cancel:&AtomicBool)->Result<Self>{
  if layers.is_empty()||layers.len()>8||layers.iter().any(Uuid::is_nil)||layers.iter().collect::<HashSet<_>>().len()!=layers.len(){return Err(Error::Invalid)}
  if store.revision()?!=revision{return Err(Error::Conflict)}
  let tiles=viewport_tiles(b,z)?;
  let dir=tempfile::tempdir_in(root).map_err(|_|Error::Storage)?;
  let path=dir.path().join("viewport.mbtiles");
  let mut db=Connection::open(&path)?;
  db.execute_batch("PRAGMA journal_mode=DELETE; PRAGMA synchronous=FULL; CREATE TABLE metadata(name TEXT PRIMARY KEY,value TEXT NOT NULL); CREATE TABLE tiles(zoom_level INTEGER NOT NULL,tile_column INTEGER NOT NULL,tile_row INTEGER NOT NULL,tile_data BLOB NOT NULL,PRIMARY KEY(zoom_level,tile_column,tile_row));")?;
  let tx=db.transaction_with_behavior(TransactionBehavior::Immediate)?;
  let fields=serde_json::json!({"vector_layers":layers.iter().map(|id|serde_json::json!({"id":id.to_string(),"fields":{"stable_id":"String"},"minzoom":z,"maxzoom":z})).collect::<Vec<_>>()});
  let zoom=z.to_string();let bounds=format!("{},{},{},{}",b[0],b[1],b[2],b[3]);let meta=json(&fields)?;
  for (k,v) in [("name","OpenVietGIS viewport"),("format","pbf"),("type","overlay"),("version","1.3"),("minzoom",zoom.as_str()),("maxzoom",zoom.as_str()),("bounds",bounds.as_str()),("json",meta.as_str())]{
   tx.execute("INSERT INTO metadata VALUES(?1,?2)",params![k,v])?;
  }
  let mut bytes=0usize;
  for &(x,y) in &tiles{
   if cancel.load(Ordering::Acquire){return Err(Error::Conflict)}
   // MVT Tile is a sequence of repeated layer messages; concatenation preserves them.
   let mut tile=Vec::new();
   for l in layers{
    let part=mvt::named_tile(store,*l,revision,z,x,y,&l.to_string(),cancel).map_err(|e|match e{"STALE_REVISION"|"CANCELLED"=>Error::Conflict,"RESOURCE_LIMIT"|"TILE_DENSITY_LIMIT"=>Error::Limit,_=>Error::Invalid})?;
    if tile.len()+part.len()>4*1024*1024{return Err(Error::Limit)}
    tile.extend(part);
   }
   let mut zip=GzEncoder::new(Vec::new(),Compression::fast());zip.write_all(&tile).map_err(|_|Error::Storage)?;
   let data=zip.finish().map_err(|_|Error::Storage)?;
   bytes=bytes.checked_add(data.len()).ok_or(Error::Limit)?;if bytes>MAX_BYTES{return Err(Error::Limit)}
   let tms=(1u32<<z)-1-y;
   tx.execute("INSERT INTO tiles VALUES(?1,?2,?3,?4)",params![z,x,tms,data])?;
  }
  if cancel.load(Ordering::Acquire)||store.revision()?!=revision{return Err(Error::Conflict)}
  tx.commit()?;drop(db);
  if std::fs::metadata(&path).map_err(|_|Error::Storage)?.len()>MAX_BYTES as u64+2*1024*1024{return Err(Error::Limit)}
  Ok(Self{info:ArchiveInfo{path,revision:revision.to_string(),zoom:z,tiles:tiles.len(),bytes,layers:layers.to_vec()},_dir:dir})
 }
}
#[cfg(test)]mod tests{
 use super::*;
 use std::io::Read;
 #[test]fn viewport_limits(){assert_eq!(viewport_tiles([-180.,-85.,180.,85.],0).unwrap(),vec![(0,0)]);assert!(viewport_tiles([-180.,-85.,180.,85.],10).is_err());assert!(viewport_tiles([179.,0.,-179.,1.],5).is_err());}
 #[test]fn archive_multilayer_and_tms(){
  let d=tempfile::tempdir().unwrap();let store=Store::create(&d.path().join("p.sqlite")).unwrap();let a=Uuid::new_v4();let b=Uuid::new_v4();
  store.add_layer(a,"A",0).unwrap();store.add_layer(b,"B",1).unwrap();
  let id=Uuid::new_v4();store.apply(&Batch{operation:Uuid::new_v4(),expected_project:2,edits:vec![Edit{layer:a,feature:id,expected:0,image:Some(Image{wkb:ovg_mobile_engine::edit::draft("point",&[106.,11.]).unwrap(),attributes:serde_json::json!({})})}],links:vec![]}).unwrap();
  let cancel=AtomicBool::new(false);let arc=TileArchive::build(&store,&[a,b],3,[105.99,10.99,106.01,11.01],8,d.path(),&cancel).unwrap();
  let db=Connection::open(&arc.info.path).unwrap();let (x,y)=xyz(106.,11.,8);let tms=255-y;
  let raw:Vec<u8>=db.query_row("SELECT tile_data FROM tiles WHERE zoom_level=8 AND tile_column=?1 AND tile_row=?2",params![x,tms],|r|r.get(0)).unwrap();
  let mut bytes=Vec::new();flate2::read::GzDecoder::new(raw.as_slice()).read_to_end(&mut bytes).unwrap();
  let mut expected=mvt::named_tile(&store,a,3,8,x,y,&a.to_string(),&cancel).unwrap();expected.extend(mvt::named_tile(&store,b,3,8,x,y,&b.to_string(),&cancel).unwrap());
  assert_eq!(bytes,expected);assert!(bytes.windows(36).any(|w|w==id.to_string().as_bytes()));
  assert!(TileArchive::build(&store,&[a],2,[105.,10.,107.,12.],8,d.path(),&cancel).is_err());
  let path=arc.info.path.clone();drop(db);drop(arc);assert!(!path.exists());
 }
}
