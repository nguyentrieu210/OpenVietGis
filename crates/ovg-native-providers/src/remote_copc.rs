//! Bounded COPC reader: LAS 1.4 point format 6, LASzip v3 Point14 only.
//! Read selected hierarchy pages and node chunks, never arbitrary EVLR allocations.
use std::{io::{self,Read,Seek,SeekFrom},collections::{HashSet,VecDeque},sync::atomic::{AtomicBool,Ordering}};
use serde::Serialize;
use laz::{LazVlr,record::{LayeredPointRecordDecompressor,RecordDecompressor}};
const MAX_PAGE:u64=1024*1024;
const MAX_PAGES:usize=64;
const MAX_NODES:usize=32768;
const MAX_POINTS:usize=2048;
fn bad()->io::Error{io::Error::new(io::ErrorKind::InvalidData,"UNSUPPORTED_OR_INVALID_COPC")}
fn limit()->io::Error{io::Error::new(io::ErrorKind::InvalidData,"COPC_DENSITY_OR_RESOURCE_LIMIT")}
fn u16v(b:&[u8])->u16{u16::from_le_bytes(b[..2].try_into().unwrap())}
fn u32v(b:&[u8])->u32{u32::from_le_bytes(b[..4].try_into().unwrap())}
fn i32v(b:&[u8])->i32{i32::from_le_bytes(b[..4].try_into().unwrap())}
fn u64v(b:&[u8])->u64{u64::from_le_bytes(b[..8].try_into().unwrap())}
fn f64v(b:&[u8])->f64{f64::from_le_bytes(b[..8].try_into().unwrap())}
fn at<R:Read+Seek>(r:&mut R,len:u64,off:u64,n:usize)->io::Result<Vec<u8>>{
 if n>MAX_PAGE as usize||off.checked_add(n as u64).is_none_or(|e|e>len){return Err(bad())}
 let mut b=vec![0;n];r.seek(SeekFrom::Start(off))?;r.read_exact(&mut b)?;Ok(b)
}
#[derive(Clone,Copy,Debug,PartialEq,Eq,Hash)]
struct Key{level:i32,xyz:[i32;3]}
impl Key{
 fn read(b:&[u8])->io::Result<Self>{
  let level=i32v(b);let xyz=[i32v(&b[4..]),i32v(&b[8..]),i32v(&b[12..])];
  if !(0..=24).contains(&level)||xyz.iter().any(|&n|n<0||n>=(1i32<<level)){return Err(bad())}Ok(Self{level,xyz})
 }
 fn descendant(self,parent:Self)->bool{self.level>=parent.level&&(0..3).all(|i|self.xyz[i]>>(self.level-parent.level)==parent.xyz[i])}
}
#[derive(Clone,Debug,Serialize)]
pub struct Info{pub center:[f64;3],pub halfsize:f64,pub spacing:f64,pub points:u64,pub point_format:u8}
pub struct Copc{pub info:Info,scale:[f64;3],offset:[f64;3],root:u64,root_size:u64,point_start:u64,len:u64,vlr:Vec<u8>}
#[derive(Clone,Copy)]
struct Node{key:Key,offset:u64,size:usize,points:usize}
impl Copc{
 pub fn open<R:Read+Seek>(r:&mut R,len:u64)->io::Result<Self>{
  let h=at(r,len,0,375)?;
  if &h[..4]!=b"LASF"||h[24]!=1||h[25]!=4||h[104]!=0x86||u16v(&h[105..])!=30{return Err(bad())}
  let header=u16v(&h[94..])as u64;let point_start=u32v(&h[96..])as u64;let vlrs=u32v(&h[100..]);
  if header<375||header>point_start||point_start>MAX_PAGE||vlrs==0||vlrs>64{return Err(bad())}
  let points=u64v(&h[247..]);if points==0||points>1_000_000_000{return Err(limit())}
  let scale=std::array::from_fn(|i|f64v(&h[131+i*8..]));let offset=std::array::from_fn(|i|f64v(&h[155+i*8..]));
  if scale.iter().any(|v|!v.is_finite()||*v<=0.)||offset.iter().any(|v|!v.is_finite()){return Err(bad())}
  let mut pos=header;let mut copc=None;let mut vlr=None;
  for _ in 0..vlrs{
   let v=at(r,len,pos,54)?;let id=u16v(&v[18..]);let n=u16v(&v[20..])as usize;
   pos=pos.checked_add(54).ok_or_else(bad)?;
   if pos.checked_add(n as u64).is_none_or(|e|e>point_start){return Err(bad())}
   let user=&v[2..18];let user=&user[..user.iter().position(|&x|x==0).unwrap_or(user.len())];
   match (user,id){
    (b"copc",1)=>{if copc.is_some()||n!=160{return Err(bad())}copc=Some(at(r,len,pos,n)?);},
    (b"laszip encoded",22204)=>{if vlr.is_some()||n!=40{return Err(bad())}vlr=Some(at(r,len,pos,n)?);},
    _=>{}
   }
   pos+=n as u64;
  }
  let c=copc.ok_or_else(bad)?;let vlr=vlr.ok_or_else(bad)?;
  // Layered chunked compressor 3, arithmetic coder 0, one Point14 item v3.
  if u16v(&vlr)!=3||u16v(&vlr[2..])!=0||u16v(&vlr[32..])!=1||u16v(&vlr[34..])!=10||u16v(&vlr[36..])!=30||u16v(&vlr[38..])!=3{return Err(bad())}
  let center=std::array::from_fn(|i|f64v(&c[i*8..]));let halfsize=f64v(&c[24..]);let spacing=f64v(&c[32..]);let root=u64v(&c[40..]);let root_size=u64v(&c[48..]);
  if center.iter().any(|v|!v.is_finite())||!halfsize.is_finite()||halfsize<=0.||!spacing.is_finite()||spacing<=0.||center.iter().any(|v|!(v+halfsize).is_finite()||!(v-halfsize).is_finite())||root<point_start||root_size==0||root_size>MAX_PAGE||root_size%32!=0||root.checked_add(root_size).is_none_or(|e|e>len){return Err(bad())}
  Ok(Self{info:Info{center,halfsize,spacing,points,point_format:6},scale,offset,root,root_size,point_start,len,vlr})
 }
 fn intersects(&self,k:Key,b:[f64;6])->bool{
  let size=self.info.halfsize*2./f64::from(1u32<<k.level);
  (0..3).all(|i|{let lo=self.info.center[i]-self.info.halfsize+f64::from(k.xyz[i])*size;lo<=b[i+3]&&lo+size>=b[i]})
 }
 fn selected<R:Read+Seek>(&self,r:&mut R,b:[f64;6],level:i32,cancel:&AtomicBool)->io::Result<Vec<Node>>{
  if !(0..=16).contains(&level)||b.iter().any(|v|!v.is_finite())||(0..3).any(|i|b[i]>b[i+3]){return Err(bad())}
  let root=Key{level:0,xyz:[0;3]};
  let mut todo=VecDeque::from([(root,self.root,self.root_size)]);let mut pages=HashSet::new();let mut keys=HashSet::new();let mut nodes=Vec::new();let mut points=0usize;let mut examined=0usize;
  while let Some((parent,off,size))=todo.pop_front(){
   if cancel.load(Ordering::Acquire){return Err(io::Error::new(io::ErrorKind::Interrupted,"CANCELLED"))}
   if pages.len()>=MAX_PAGES||!pages.insert(off)||off<self.point_start||size==0||size>MAX_PAGE||size%32!=0{return Err(limit())}
   let page=at(r,self.len,off,size as usize)?;
   for e in page.chunks_exact(32){
    examined+=1;if examined>MAX_NODES{return Err(limit())}
    let k=Key::read(e)?;let o=u64v(&e[16..]);let n=i32v(&e[24..]);let count=i32v(&e[28..]);
    if !k.descendant(parent)||count< -1||n<0{return Err(bad())}
    if count==0{if n!=0||o!=0{return Err(bad())}continue}
    if n==0||o<self.point_start||o.checked_add(n as u64).is_none_or(|end|end>self.len){return Err(bad())}
    if count==-1{
     if n as u64>MAX_PAGE||n%32!=0{return Err(bad())}
     if k.level<=level&&self.intersects(k,b){if todo.len()+pages.len()>=MAX_PAGES{return Err(limit())}todo.push_back((k,o,n as u64));}
    }else{
     if !keys.insert(k){return Err(bad())}
     if k.level==level&&self.intersects(k,b){
      points=points.checked_add(count as usize).ok_or_else(limit)?;
      if points>MAX_POINTS||n as u64>MAX_PAGE{return Err(limit())}
      nodes.push(Node{key:k,offset:o,size:n as usize,points:count as usize});
     }
    }
   }
  }
  Ok(nodes)
 }
 pub fn query<R:Read+Seek>(&self,r:&mut R,b:[f64;6],level:i32,cancel:&AtomicBool)->io::Result<Vec<u8>>{
  let nodes=self.selected(r,b,level,cancel)?;
  let vlr=LazVlr::from_buffer(&self.vlr).map_err(|_|bad())?;
  let mut out=Vec::new();
  for n in nodes{
   if cancel.load(Ordering::Acquire){return Err(io::Error::new(io::ErrorKind::Interrupted,"CANCELLED"))}
   let data=at(r,self.len,n.offset,n.size)?;
   // Point14 v3 chunk: raw 30-byte point, u32 count, nine layer lengths.
   // Preflight every allocation before constructing the third-party decoder.
   if data.len()<70||u32v(&data[30..])as usize!=n.points{return Err(bad())}
   let mut total=70usize;
   for i in 0..9{total=total.checked_add(u32v(&data[34+i*4..])as usize).ok_or_else(bad)?;}
   if total!=data.len(){return Err(bad())}
   let decoded=std::panic::catch_unwind(std::panic::AssertUnwindSafe(||->io::Result<Vec<[f64;3]>>{
    let mut d=LayeredPointRecordDecompressor::new(io::Cursor::new(data));d.set_fields_from(vlr.items()).map_err(|_|bad())?;
    let mut raw=[0u8;30];let mut pts=Vec::new();
    for _ in 0..n.points{
     if cancel.load(Ordering::Acquire){return Err(io::Error::new(io::ErrorKind::Interrupted,"CANCELLED"))}
     d.decompress_next(&mut raw)?;
     let p=std::array::from_fn(|i|f64::from(i32v(&raw[i*4..]))*self.scale[i]+self.offset[i]);
     if p.iter().any(|v|!v.is_finite()){return Err(bad())}
     if (0..3).all(|i|p[i]>=b[i]&&p[i]<=b[i+3]){pts.push(p)}
    }Ok(pts)
   })).map_err(|_|bad())??;
   for p in decoded{for v in p{out.extend(v.to_le_bytes())}}
   let _=n.key;
  }
  Ok(out)
 }
}
#[cfg(test)]
mod tests{
 use super::*;
 #[test]fn rejects_bad_key_and_header(){
  let mut k=[0u8;16];k[..4].copy_from_slice(&25i32.to_le_bytes());assert!(Key::read(&k).is_err());
  assert!(Copc::open(&mut io::Cursor::new(vec![0;375]),375).is_err());
 }
 #[test]fn cyclic_and_excessive_hierarchy_rejected(){
  let c=Copc{info:Info{center:[0.;3],halfsize:10.,spacing:1.,points:1,point_format:6},scale:[1.;3],offset:[0.;3],root:64,root_size:32,point_start:32,len:128,vlr:vec![]};
  let mut b=vec![0u8;128];b[80..88].copy_from_slice(&64u64.to_le_bytes());b[88..92].copy_from_slice(&32i32.to_le_bytes());b[92..96].copy_from_slice(&(-1i32).to_le_bytes());
  assert!(c.selected(&mut io::Cursor::new(b),[-10.,-10.,-10.,10.,10.,10.],0,&AtomicBool::new(false)).is_err());
 }
}
