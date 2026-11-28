//! Reference untextured glTF 2.0 binary terrain export.
//! Converts local projected Z-up to glTF Y-up; retains origin/CRS in extras.
//! Not a georeferenced 3D Tiles tileset; no material texture or atlas.
use super::mesh::Mesh;
use serde_json::json;
#[derive(Debug,Clone,Copy,PartialEq,Eq)]
pub enum Error{Invalid,Limit,Serialization}
pub fn encode(mesh:&Mesh,epsg:u16,max_bytes:u64)->Result<Vec<u8>,Error>{
 if epsg==0||epsg>=32767||mesh.vertices.is_empty()||mesh.triangles.is_empty()||
 !mesh.origin.iter().all(|v|v.is_finite()){return Err(Error::Invalid);}
 if mesh.vertices.len()>4_000_000||mesh.triangles.len()>8_000_000{return Err(Error::Limit);}
 let nv=mesh.vertices.len();let ni=mesh.triangles.len().checked_mul(3).ok_or(Error::Limit)?;
 let pos=nv.checked_mul(12).ok_or(Error::Limit)?;let uv=nv.checked_mul(8).ok_or(Error::Limit)?;
 let idx=ni.checked_mul(4).ok_or(Error::Limit)?;
 let size=pos.checked_add(uv).and_then(|n|n.checked_add(idx)).ok_or(Error::Limit)?;
 if size as u64>max_bytes||size>256*1024*1024{return Err(Error::Limit);}
 let mut lo=[f32::INFINITY;3];let mut hi=[f32::NEG_INFINITY;3];let mut bin=Vec::with_capacity(size);
 for v in &mesh.vertices{
 if !v.xyz.iter().chain(v.uv.iter()).all(|x|x.is_finite())||v.uv.iter().any(|x|!(0.0..=1.0).contains(x)){return Err(Error::Invalid);}
 let p=[v.xyz[0],v.xyz[2],-v.xyz[1]];
 for i in 0..3{lo[i]=lo[i].min(p[i]);hi[i]=hi[i].max(p[i]);bin.extend(p[i].to_le_bytes());}
 }
 for v in &mesh.vertices{for x in v.uv{bin.extend(x.to_le_bytes());}}
 for t in &mesh.triangles{
 if t[0]==t[1]||t[1]==t[2]||t[2]==t[0]||t.iter().any(|i|*i as usize>=nv){return Err(Error::Invalid);}
 for i in t{bin.extend(i.to_le_bytes());}}
 let doc=json!({"asset":{"version":"2.0","generator":"OpenVietGIS Photogrammetry"},
 "scene":0,"scenes":[{"nodes":[0]}],"nodes":[{"mesh":0,"extras":{"projectedOrigin":mesh.origin,"epsg":epsg,"localAxes":"east,up,-north"}}],
 "meshes":[{"primitives":[{"attributes":{"POSITION":0,"TEXCOORD_0":1},"indices":2,"mode":4}]}],
 "buffers":[{"byteLength":size}],
 "bufferViews":[{"buffer":0,"byteOffset":0,"byteLength":pos,"target":34962},
 {"buffer":0,"byteOffset":pos,"byteLength":uv,"target":34962},
 {"buffer":0,"byteOffset":pos+uv,"byteLength":idx,"target":34963}],
 "accessors":[{"bufferView":0,"componentType":5126,"count":nv,"type":"VEC3","min":lo,"max":hi},
 {"bufferView":1,"componentType":5126,"count":nv,"type":"VEC2"},
 {"bufferView":2,"componentType":5125,"count":ni,"type":"SCALAR"}]});
 let mut j=serde_json::to_vec(&doc).map_err(|_|Error::Serialization)?;
 while j.len()%4!=0{j.push(b' ');}
 let total=28usize.checked_add(j.len()).and_then(|n|n.checked_add(bin.len())).ok_or(Error::Limit)?;
 if total as u64>max_bytes||total>256*1024*1024{return Err(Error::Limit);}
 let mut out=Vec::with_capacity(total);
 out.extend(b"glTF");out.extend(2u32.to_le_bytes());out.extend((total as u32).to_le_bytes());
 out.extend((j.len() as u32).to_le_bytes());out.extend(b"JSON");out.extend(j);
 out.extend((bin.len() as u32).to_le_bytes());out.extend(b"BIN\0");out.extend(bin);
 Ok(out)
}
#[cfg(test)]mod tests{
 use super::*;use super::super::{mesh::terrain,ortho::Grid};
 fn sample()->Mesh{terrain(Grid{origin_x:500000.,origin_y:1200000.,gsd:1.,width:2,height:2},&[10.;4],10.,4,2,2.).unwrap()}
 #[test]fn glb_structure_and_coordinate_metadata(){
 let b=encode(&sample(),32648,4096).unwrap();assert_eq!(&b[..4],b"glTF");
 assert_eq!(u32::from_le_bytes(b[8..12].try_into().unwrap()) as usize,b.len());
 let n=u32::from_le_bytes(b[12..16].try_into().unwrap()) as usize;
 let j:serde_json::Value=serde_json::from_slice(&b[20..20+n]).unwrap();
 assert_eq!(j["nodes"][0]["extras"]["epsg"],32648);assert_eq!(j["accessors"][2]["count"],6);
 assert_eq!(&b[24+n..28+n],b"BIN\0");}
 #[test]fn rejects_invalid_indices_and_budget(){
 let mut m=sample();m.triangles[0][0]=100;
 assert_eq!(encode(&m,32648,4096),Err(Error::Invalid));
 assert_eq!(encode(&sample(),32648,10),Err(Error::Limit));}
}
