//! Desktop read-only resource adapters. These are deliberately narrow interchange
//! profiles, not OpenVietCAD/COPC/3D Tiles renderer replacements.
use serde_json::{json,Value};
use std::{fs::File,io::{Read,Seek,SeekFrom,Write},path::Path};
type Result<T>=std::result::Result<T,&'static str>;
const CHUNK:u64=2048;
const MAX_FILE:u64=512*1024*1024;
const MAX_SMALL:u64=4*1024*1024;
fn source(p:&Path)->Result<File>{
 if !std::fs::symlink_metadata(p).map_err(|_|"STORAGE_UNAVAILABLE")?.file_type().is_file(){return Err("INVALID_FILE")}
 File::open(p).map_err(|_|"STORAGE_UNAVAILABLE")
}
fn small(p:&Path)->Result<Vec<u8>>{
 let mut b=Vec::new();source(p)?.take(MAX_SMALL+1).read_to_end(&mut b).map_err(|_|"STORAGE_UNAVAILABLE")?;
 if b.len() as u64>MAX_SMALL{return Err("RESOURCE_LIMIT")}Ok(b)
}
fn le16(b:&[u8],i:usize)->u16{u16::from_le_bytes(b[i..i+2].try_into().unwrap())}
fn le32(b:&[u8],i:usize)->u32{u32::from_le_bytes(b[i..i+4].try_into().unwrap())}
fn le64(b:&[u8],i:usize)->u64{u64::from_le_bytes(b[i..i+8].try_into().unwrap())}
fn f64le(b:&[u8],i:usize)->f64{f64::from_bits(le64(b,i))}
fn xy(x:f64,y:f64)->bool{x.is_finite()&&y.is_finite()&&x.abs()<=180.&&y.abs()<=90.}
pub struct Cad { features:Vec<Value> }
impl Cad{
 // WGS84 XY must be explicitly asserted by the caller. No implicit CAD CRS.
 pub fn open(path:&Path)->Result<Self>{Self::open_profile(path,false)}
 pub fn open_linked(path:&Path)->Result<Self>{Self::open_profile(path,true)}
 fn open_profile(path:&Path,allow_empty:bool)->Result<Self>{
   let b=small(path)?;let s=std::str::from_utf8(&b).map_err(|_|"UNSUPPORTED_DXF")?;
   let lines:Vec<_>=s.lines().collect();if lines.len()%2!=0||lines.len()>200_000{return Err("INVALID_DXF")}
   let mut pairs=Vec::new();for p in lines.chunks_exact(2){pairs.push((p[0].trim().parse::<u16>().map_err(|_|"INVALID_DXF")?,p[1].trim()));}
   let mut entities=false;let mut found=false;let mut out=Vec::new();let mut handles=std::collections::HashSet::new();let mut i=0;
   while i<pairs.len(){
     if pairs[i]==(0,"SECTION")&&pairs.get(i+1)==Some(&(2,"ENTITIES")){if allow_empty&&found{return Err("INVALID_DXF")}entities=true;found=true;i+=2;continue}
     if pairs[i]==(0,"ENDSEC"){entities=false;i+=1;continue}
     if !entities{i+=1;continue}
     let (code,kind)=pairs[i];if code!=0{return Err("INVALID_DXF")}
     let start=i+1;i=start;while i<pairs.len()&&pairs[i].0!=0{i+=1;}
     let fields=&pairs[start..i];
     if !matches!(kind,"POINT"|"LINE"){return Err("UNSUPPORTED_DXF_ENTITY")}
     if allow_empty&&fields.iter().any(|(c,v)|(*c==67&&*v!="0")||(*c==410&&!v.eq_ignore_ascii_case("Model"))){return Err("PAPER_SPACE_UNSUPPORTED")}
     let field=|c:u16|->Result<&str>{let mut v=fields.iter().filter(|p|p.0==c);let f=v.next().ok_or("INVALID_DXF")?.1;if v.next().is_some(){return Err("INVALID_DXF")}Ok(f)};
     let num=|c|->Result<f64>{field(c)?.parse::<f64>().ok().filter(|v|v.is_finite()).ok_or("INVALID_DXF")};
     let a=[num(10)?,num(20)?];if !xy(a[0],a[1]){return Err("INVALID_CRS_COORDINATE")}
     for c in [30,31]{if let Some((_,v))=fields.iter().find(|p|p.0==c){if v.parse::<f64>().ok()!=Some(0.){return Err("UNSUPPORTED_DXF_Z")}}}
     // Reject non-default OCS extrusion rather than silently changing geometry.
     for (c,default) in [(210,0.),(220,0.),(230,1.)]{if let Some((_,v))=fields.iter().find(|p|p.0==c){if v.parse::<f64>().ok()!=Some(default){return Err("UNSUPPORTED_DXF_OCS")}}}
     let geom=if kind=="POINT"{json!({"type":"Point","coordinates":a})}else{let z=[num(11)?,num(21)?];if !xy(z[0],z[1]){return Err("INVALID_CRS_COORDINATE")}json!({"type":"LineString","coordinates":[a,z]})};
     let handle=field(5)?.to_ascii_uppercase();let layer=field(8)?;
     if allow_empty&&(handle.starts_with('0')||u64::from_str_radix(&handle,16).ok().is_none()){return Err("INVALID_DXF_IDENTITY")}
     if handle.is_empty()||handle.len()>16||!handle.bytes().all(|b|b.is_ascii_hexdigit())||!handles.insert(handle.clone())||layer.is_empty()||layer.len()>256||layer.chars().any(char::is_control){return Err("INVALID_DXF_IDENTITY")}
     out.push(json!({"type":"Feature","geometry":geom,"properties":{"handle":handle,"layer":layer,"entity":kind}}));
     if out.len()>8192{return Err("RESOURCE_LIMIT")}
   }
   if !found||entities||(allow_empty&&pairs.last()!=Some(&(0,"EOF")))||(!allow_empty&&out.is_empty()){return Err("INVALID_DXF")}Ok(Self{features:out})
 }
 pub fn viewport(&self,b:[f64;4])->Result<Vec<u8>>{
   if b.iter().any(|n|!n.is_finite())||b[0]>b[2]||b[1]>b[3]||!xy(b[0],b[1])||!xy(b[2],b[3]){return Err("INVALID_BBOX")}
   let mut rows=Vec::new();
   for f in &self.features{
     let g=&f["geometry"];let c=&g["coordinates"];
     let coord=|p:&Value|->Option<(f64,f64)>{Some((p[0].as_f64()?,p[1].as_f64()?))};
     let Some(a)=(if g["type"]=="Point"{coord(c)}else{coord(&c[0])})else{return Err("INVALID_DXF")};
     let z=if g["type"]=="Point"{a}else{coord(&c[1]).ok_or("INVALID_DXF")?};
     if a.0.min(z.0)<=b[2]&&a.0.max(z.0)>=b[0]&&a.1.min(z.1)<=b[3]&&a.1.max(z.1)>=b[1]{
       if rows.len()==512{return Err("VIEWPORT_DENSITY_LIMIT")}rows.push(f);
     }
   }
   let out=serde_json::to_vec(&json!({"type":"FeatureCollection","features":rows})).map_err(|_|"INVALID_DXF")?;
   if out.len()>2*1024*1024{return Err("RESOURCE_LIMIT")}Ok(out)
 }
 pub fn features(&self)->&[Value]{&self.features}
 pub fn count(&self)->u64{self.features.len()as u64}
 pub fn chunk(&self,n:u32)->Result<Vec<u8>>{
  let start=(n as usize).checked_mul(CHUNK as usize).ok_or("INVALID_CHUNK")?;
  let rows=self.features.get(start..self.features.len().min(start.saturating_add(CHUNK as usize))).ok_or("INVALID_CHUNK")?;
  if rows.is_empty(){return Err("INVALID_CHUNK")}
  serde_json::to_vec(&json!({"type":"FeatureCollection","features":rows})).map_err(|_|"INVALID_DXF")
 }
}
pub struct Las{ file:File,offset:u64,record:u64,count:u64,scale:[f64;3],origin:[f64;3] }
impl Las{
 // Snapshot to private anonymous tempfile so revision cannot change externally.
 pub fn open(path:&Path)->Result<Self>{
   let mut src=source(path)?;let size=src.metadata().map_err(|_|"STORAGE_UNAVAILABLE")?.len();
   if !(227..=MAX_FILE).contains(&size){return Err("RESOURCE_LIMIT")}
   let mut f=tempfile::tempfile().map_err(|_|"STORAGE_UNAVAILABLE")?;
   let copied=std::io::copy(&mut src.take(MAX_FILE+1),&mut f).map_err(|_|"STORAGE_UNAVAILABLE")?;
   if copied!=size{return Err("SOURCE_CHANGED")}f.flush().map_err(|_|"STORAGE_UNAVAILABLE")?;f.seek(SeekFrom::Start(0)).map_err(|_|"INVALID_LAS")?;
   let mut h=[0u8;227];f.read_exact(&mut h).map_err(|_|"INVALID_LAS")?;
   if &h[..4]!=b"LASF"||h[24]!=1||h[25]>3||h[104]>3{return Err("UNSUPPORTED_LAS_PROFILE")}
   let header=le16(&h,94)as u64;let offset=le32(&h,96)as u64;let record=le16(&h,105)as u64;let count=le32(&h,107)as u64;
   let minimum=[20,28,26,34][h[104]as usize];
   if header<227||offset<header||record<minimum||record>256||count==0||offset.checked_add(record.checked_mul(count).ok_or("INVALID_LAS")?).ok_or("INVALID_LAS")?>size{return Err("INVALID_LAS")}
   let scale=[f64le(&h,131),f64le(&h,139),f64le(&h,147)];let origin=[f64le(&h,155),f64le(&h,163),f64le(&h,171)];
   if scale.iter().any(|v|!v.is_finite()||*v<=0.)||origin.iter().any(|v|!v.is_finite()){return Err("INVALID_LAS")}
   Ok(Self{file:f,offset,record,count,scale,origin})
 }
 pub fn count(&self)->u64{self.count}
 pub fn chunk(&mut self,n:u32)->Result<Vec<u8>>{
   let start=u64::from(n)*CHUNK;if start>=self.count{return Err("INVALID_CHUNK")}
   let count=(self.count-start).min(CHUNK);
   self.file.seek(SeekFrom::Start(self.offset+start*self.record)).map_err(|_|"INVALID_LAS")?;
   let mut b=vec![0;self.record as usize];let mut out=Vec::with_capacity((count*24)as usize);
   for _ in 0..count{
     self.file.read_exact(&mut b).map_err(|_|"INVALID_LAS")?;
     for i in 0..3{let n=i32::from_le_bytes(b[i*4..i*4+4].try_into().unwrap());let v=n as f64*self.scale[i]+self.origin[i];if !v.is_finite(){return Err("INVALID_LAS")}out.extend(v.to_le_bytes());}
   }
   Ok(out)
 }
}
pub struct Glb{ bytes:Vec<u8> }
impl Glb{
 pub fn open(path:&Path)->Result<Self>{
   let b=small(path)?;
   if b.len()<20||&b[..4]!=b"glTF"||le32(&b,4)!=2||le32(&b,8)as usize!=b.len()||le32(&b,16)!=0x4e4f534a{return Err("INVALID_GLB")}
   let n=le32(&b,12)as usize;let end=20usize.checked_add(n).ok_or("INVALID_GLB")?;
   let v:Value=serde_json::from_slice(b.get(20..end).ok_or("INVALID_GLB")?).map_err(|_|"INVALID_GLB")?;
   fn external(v:&Value)->bool{match v{Value::Object(o)=>o.contains_key("uri")||o.values().any(external),Value::Array(a)=>a.iter().any(external),_=>false}}
   if external(&v){return Err("EXTERNAL_RESOURCE_UNSUPPORTED")}
   if end<b.len()&&(b.len()<end+8||le32(&b,end+4)!=0x004e4942||end+8+le32(&b,end)as usize!=b.len()){return Err("INVALID_GLB")}
   Ok(Self{bytes:b})
 }
 pub fn len(&self)->u64{self.bytes.len()as u64}
 pub fn bytes(&self)->Vec<u8>{self.bytes.clone()}
}
