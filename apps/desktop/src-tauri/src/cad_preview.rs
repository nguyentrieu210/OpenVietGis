//! Bounded native-coordinate DXF preview. Never used for authoritative writes.
use serde::Serialize;
use std::{collections::{HashMap,HashSet},f64::consts::TAU,io::Read,path::Path};
type R<T>=Result<T,&'static str>;
type XY=[f64;2];
type Pair<'a>=(u16,&'a str);
const MAX_POINTS:usize=65536;
#[derive(Debug,Serialize,PartialEq)]
pub struct Stroke{pub handle:String,pub layer:String,pub kind:String,pub paths:Vec<Vec<XY>>,pub fill:bool}
fn field<'a>(f:&[Pair<'a>],code:u16)->R<&'a str>{
 let mut v=f.iter().filter(|p|p.0==code);let s=v.next().ok_or("INVALID_DXF")?.1;
 if v.next().is_some(){return Err("INVALID_DXF")}Ok(s)
}
fn num(s:&str)->R<f64>{s.parse::<f64>().ok().filter(|v|v.is_finite()&&v.abs()<=1e12).ok_or("INVALID_COORDINATE")}
fn get(f:&[Pair<'_>],c:u16)->R<f64>{num(field(f,c)?)}
fn opt(f:&[Pair<'_>],c:u16,d:f64)->R<f64>{if f.iter().any(|p|p.0==c){get(f,c)}else{Ok(d)}}
fn xy(f:&[Pair<'_>],a:u16,b:u16)->R<XY>{Ok([get(f,a)?,get(f,b)?])}
fn ocs(f:&[Pair<'_>])->R<()>{
 for(c,d)in [(30,0.),(31,0.),(38,0.),(39,0.),(210,0.),(220,0.),(230,1.)]{if opt(f,c,d)?!=d{return Err("UNSUPPORTED_3D_DXF")}}Ok(())
}
fn poly(f:&[Pair<'_>],count:usize,closed:bool)->R<Vec<XY>>{
 if !(2..=4096).contains(&count){return Err("POLYLINE_LIMIT")}
 let mut out=Vec::new();let mut x=None;
 for &(c,v) in f{match c{
  10=>{if x.is_some(){return Err("INVALID_POLYLINE")}x=Some(num(v)?);},
  20=>out.push([x.take().ok_or("INVALID_POLYLINE")?,num(v)?]),
  42|40|41=>if num(v)?!=0.{return Err("UNSUPPORTED_BULGE_OR_WIDTH")},
  _=>{}
 }}
 if x.is_some()||out.len()!=count{return Err("INVALID_POLYLINE")}
 if closed{out.push(out[0]);}Ok(out)
}
fn arc(f:&[Pair<'_>],circle:bool)->R<Vec<XY>>{
 let p=xy(f,10,20)?;let r=get(f,40)?;if r<=0.{return Err("INVALID_RADIUS")}
 let start=if circle{0.}else{get(f,50)?.to_radians()};
 let sweep=if circle{TAU}else{(get(f,51)?.to_radians()-start).rem_euclid(TAU)};
 if sweep<=1e-12{return Err("INVALID_ARC")}
 let n=((sweep/TAU*128.).ceil()as usize).max(2);
 let v=(0..=n).map(|i|{let a=start+sweep*i as f64/n as f64;[p[0]+r*a.cos(),p[1]+r*a.sin()]}).collect();Ok(v)
}
fn hatch(f:&[Pair<'_>])->R<Vec<Vec<XY>>>{
 // Only solid, non-associative, closed, straight polyline boundaries.
 if get(f,70)?!=1.||opt(f,71,0.)?!=0.{return Err("UNSUPPORTED_HATCH")}
 let loops=get(f,91)?;if loops.fract()!=0.||!(1. ..=32.).contains(&loops){return Err("HATCH_LIMIT")}
 let starts:Vec<_>=f.iter().enumerate().filter_map(|(i,p)|(p.0==92).then_some(i)).collect();
 if starts.len()!=loops as usize{return Err("INVALID_HATCH")}
 let mut result=Vec::new();
 for(i,&s)in starts.iter().enumerate(){
  let end=starts.get(i+1).copied().unwrap_or(f.len());let row=&f[s..end];
  let flags=get(row,92)?;if flags.fract()!=0.||flags<0.||(flags as u32&2)==0{return Err("UNSUPPORTED_HATCH_EDGE")}
  if get(row,72)?!=0.||get(row,73)?!=1.{return Err("UNSUPPORTED_HATCH_EDGE")}
  let n=get(row,93)?;if n.fract()!=0.||!(3. ..=4096.).contains(&n){return Err("HATCH_LIMIT")}
  let start=row.iter().position(|p|p.0==93).ok_or("INVALID_HATCH")?+1;
  let stop=row[start..].iter().position(|p|p.0==97).ok_or("INVALID_HATCH")?+start;
  if num(row[stop].1)?!=0.{return Err("UNSUPPORTED_ASSOCIATIVE_HATCH")}
  result.push(poly(&row[start..stop],n as usize,true)?);
 }Ok(result)
}
#[derive(Clone)]
struct Block<'a>{base:XY,entities:Vec<(&'a str,Vec<Pair<'a>>)>}
fn render(kind:&str,f:&[Pair<'_>],blocks:&HashMap<String,Block<'_>>,stack:&mut HashSet<String>,budget:&mut usize)->R<Vec<Vec<XY>>>{
 ocs(f)?;
 let paths=match kind{
 "POINT"=>vec![vec![xy(f,10,20)?]],
 "LINE"=>vec![vec![xy(f,10,20)?,xy(f,11,21)?]],
 "LWPOLYLINE"=>{let n=get(f,90)?;let flags=opt(f,70,0.)?;if n.fract()!=0.||!(2. ..=4096.).contains(&n)||flags.fract()!=0.||flags<0.||flags>129.||opt(f,43,0.)?!=0.{return Err("UNSUPPORTED_POLYLINE")}
 vec![poly(f,n as usize,(flags as u32&1)!=0)?]},
 "ARC"=>vec![arc(f,false)?],
 "CIRCLE"=>vec![arc(f,true)?],
 "HATCH"=>hatch(f)?,
 "INSERT"=>{
  if opt(f,66,0.)?!=0.||opt(f,70,1.)?!=1.||opt(f,71,1.)?!=1.{return Err("UNSUPPORTED_INSERT_ARRAY")}
  let name=field(f,2)?.to_ascii_uppercase();
  if stack.len()>=8||!stack.insert(name.clone()){return Err("BLOCK_RECURSION")}
  let b=blocks.get(&name).ok_or("MISSING_BLOCK")?;
  let p=xy(f,10,20)?;let sx=opt(f,41,1.)?;let sy=opt(f,42,1.)?;
  if sx==0.||sy==0.||opt(f,43,1.)?!=1.{return Err("UNSUPPORTED_INSERT_SCALE")}
  let angle=opt(f,50,0.)?.to_radians();let(c,s)=(angle.cos(),angle.sin());
  let mut result=Vec::new();
  for (k,fields)in &b.entities{
   for mut path in render(k,fields,blocks,stack,budget)?{
    for v in &mut path{let x=(v[0]-b.base[0])*sx;let y=(v[1]-b.base[1])*sy;*v=[p[0]+c*x-s*y,p[1]+s*x+c*y];}
    result.push(path);
   }
  }
  stack.remove(&name);result
 },
 _=>return Err("UNSUPPORTED_PREVIEW_ENTITY")
 };
 for path in &paths{
  *budget=budget.checked_sub(path.len()).ok_or("PREVIEW_POINT_LIMIT")?;
  if path.iter().flatten().any(|v|!v.is_finite()||v.abs()>1e12){return Err("INVALID_COORDINATE")}
 }Ok(paths)
}
pub fn open(path:&Path)->R<Vec<Stroke>>{
 let mut bytes=Vec::new();std::fs::File::open(path).map_err(|_|"FILE_UNAVAILABLE")?.take(4*1024*1024+1).read_to_end(&mut bytes).map_err(|_|"FILE_UNAVAILABLE")?;
 if bytes.len()>4*1024*1024{return Err("RESOURCE_LIMIT")}
 let s=std::str::from_utf8(&bytes).map_err(|_|"UNSUPPORTED_DXF")?;
 let lines:Vec<_>=s.lines().collect();if lines.len()%2!=0||lines.len()>200000{return Err("INVALID_DXF")}
 let pairs:Vec<Pair<'_>>=lines.chunks_exact(2).map(|p|Ok((p[0].trim().parse().map_err(|_|"INVALID_DXF")?,p[1].trim()))).collect::<R<_>>()?;
 let mut section="";let mut block:Option<(String,Block<'_>)>=None;let mut blocks=HashMap::new();let mut entities=Vec::new();let mut i=0;
 while i<pairs.len(){
  let(code,k)=pairs[i];if code!=0{i+=1;continue}
  if k=="SECTION"{if !section.is_empty()||pairs.get(i+1).map(|p|p.0)!=Some(2){return Err("INVALID_SECTION")}section=pairs[i+1].1;i+=2;continue}
  if k=="ENDSEC"{if block.is_some(){return Err("INVALID_BLOCK")}section="";i+=1;continue}
  let start=i+1;i=start;while i<pairs.len()&&pairs[i].0!=0{i+=1}
  let f=&pairs[start..i];
  match section{
   "BLOCKS"=>match k{
    "BLOCK"=>{if block.is_some()||blocks.len()>=256{return Err("BLOCK_LIMIT")}ocs(f)?;let name=field(f,2)?.to_ascii_uppercase();block=Some((name,Block{base:xy(f,10,20)?,entities:Vec::new()}));},
    "ENDBLK"=>{let(n,b)=block.take().ok_or("INVALID_BLOCK")?;if blocks.insert(n,b).is_some(){return Err("DUPLICATE_BLOCK")}},
    _=>{let b=&mut block.as_mut().ok_or("INVALID_BLOCK")?.1;if b.entities.len()>=512{return Err("BLOCK_LIMIT")}b.entities.push((k,f.to_vec()));}
   },
   "ENTITIES"=>{if entities.len()>=512{return Err("CANVAS_ENTITY_LIMIT")}entities.push((k,f.to_vec()));},
   _=>{}
  }
 }
 if block.is_some()||!section.is_empty()||entities.is_empty(){return Err("INVALID_DXF")}
 let mut budget=MAX_POINTS;let mut seen=HashSet::new();let mut out=Vec::new();
 for(k,f)in entities{
  let handle=field(&f,5)?.to_ascii_uppercase();let layer=field(&f,8)?;
  if handle.is_empty()||handle.len()>16||!handle.bytes().all(|v|v.is_ascii_hexdigit())||!seen.insert(handle.clone())||layer.len()>256||layer.is_empty(){return Err("INVALID_DXF_IDENTITY")}
  let paths=render(k,&f,&blocks,&mut HashSet::new(),&mut budget)?;
  out.push(Stroke{handle,layer:layer.into(),kind:k.into(),paths,fill:k=="HATCH"});
 }Ok(out)
}
#[cfg(test)]mod tests{
 use super::*;
 #[test]fn arc_polyline_block_and_hatch(){
  let f=[(10,"0"),(20,"0"),(40,"2"),(50,"0"),(51,"90")];let v=arc(&f,false).unwrap();assert_eq!(v[0],[2.,0.]);assert!((v.last().unwrap()[1]-2.).abs()<1e-12);
  let p=[(10,"1"),(20,"2"),(10,"3"),(20,"4")];assert_eq!(poly(&p,2,true).unwrap(),vec![[1.,2.],[3.,4.],[1.,2.]]);
  let b=Block{base:[1.,2.],entities:vec![("POINT",vec![(10,"2"),(20,"2")])]};
  let blocks=HashMap::from([("B".into(),b)]);let insert=[(2,"B"),(10,"10"),(20,"20"),(50,"90")];
  let r=render("INSERT",&insert,&blocks,&mut HashSet::new(),&mut 100).unwrap();assert!((r[0][0][0]-10.).abs()<1e-12);assert_eq!(r[0][0][1],21.);
  let h=[(70,"1"),(91,"1"),(92,"2"),(72,"0"),(73,"1"),(93,"3"),(10,"0"),(20,"0"),(10,"1"),(20,"0"),(10,"0"),(20,"1"),(97,"0")];
  assert_eq!(hatch(&h).unwrap()[0].len(),4);
 }
}
