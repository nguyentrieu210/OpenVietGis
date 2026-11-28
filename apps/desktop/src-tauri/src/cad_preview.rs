//! Bounded native-coordinate DXF preview. Never used for authoritative writes.
use serde::{Deserialize,Serialize};
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
 if get(f,70)?!=1.||opt(f,71,0.)?!=0.||opt(f,75,0.)?!=0.{return Err("UNSUPPORTED_HATCH")}
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
   if *k=="HATCH"{return Err("BLOCK_HATCH_PREVIEW_UNSUPPORTED")}
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
    "BLOCK"=>{if block.is_some()||blocks.len()>=256{return Err("BLOCK_LIMIT")}ocs(f)?;let flags=opt(f,70,0.)?;if flags.fract()!=0.||flags<0.||flags>127.||(flags as u32&124)!=0{return Err("EXTERNAL_BLOCK_UNSUPPORTED")}let name=field(f,2)?.to_ascii_uppercase();block=Some((name,Block{base:xy(f,10,20)?,entities:Vec::new()}));},
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
/// Authoritative edits operate on native DXF group records, never on sampled
/// curves. Only the selected supported top-level entity is changed.
#[derive(Clone,Copy,Debug,Deserialize)]
#[serde(tag="type",rename_all="snake_case",deny_unknown_fields)]
pub enum CadEdit{
 Translate{dx:f64,dy:f64},
 Vertex{path:usize,vertex:usize,x:f64,y:f64},
 Radius{radius:f64},
 ArcAngles{start:f64,end:f64},
 BlockTransform{scale_x:f64,scale_y:f64,rotation:f64},
}
fn finite(v:f64)->R<f64>{if v.is_finite()&&v.abs()<=1e12{Ok(v)}else{Err("INVALID_COORDINATE")}}
#[cfg(test)]
pub fn translate(src:&Path,out:&Path,handle:&str,dx:f64,dy:f64)->R<()>{edit(src,out,handle,&CadEdit::Translate{dx,dy})}
pub fn edit(src:&Path,out:&Path,handle:&str,operation:&CadEdit)->R<()>{
 let before=open(src)?;
 let target=before.iter().find(|e|e.handle==handle).ok_or("STALE_ENTITY")?;
 let mut bytes=Vec::new();std::fs::File::open(src).map_err(|_|"FILE_UNAVAILABLE")?.take(4*1024*1024+1).read_to_end(&mut bytes).map_err(|_|"FILE_UNAVAILABLE")?;
 if bytes.len()>4*1024*1024{return Err("RESOURCE_LIMIT")}
 let raw=std::str::from_utf8(&bytes).map_err(|_|"UNSUPPORTED_DXF")?;
 let lines:Vec<_>=raw.split_inclusive('\n').collect();
 if lines.len()%2!=0{return Err("INVALID_DXF")}
 let codes:Vec<u16>=lines.chunks_exact(2).map(|p|p[0].trim().parse().map_err(|_|"INVALID_DXF")).collect::<R<_>>()?;
 let field=|i:usize|lines[i*2+1].trim();
 let mut edits=HashMap::new();let mut insertion:Option<(usize,String)>=None;let mut i=0;let mut entities=false;let mut found=false;
 while i<codes.len(){
  if codes[i]==0&&field(i)=="SECTION"&&codes.get(i+1)==Some(&2)&&field(i+1)=="ENTITIES"{entities=true;i+=2;continue}
  if codes[i]==0&&field(i)=="ENDSEC"{entities=false;i+=1;continue}
  if !entities{i+=1;continue}
  if codes[i]!=0{return Err("INVALID_DXF")}
  let kind=field(i);let start=i+1;i=start;while i<codes.len()&&codes[i]!=0{i+=1}
  if !(start..i).any(|j|codes[j]==5&&field(j).eq_ignore_ascii_case(handle)){continue}
  if found||kind!=target.kind{return Err("INVALID_DXF_IDENTITY")}found=true;
  if (start..i).any(|j|codes[j]==102){return Err("UNSUPPORTED_ENTITY_EXTENSION")}
  let stop=(start..i).find(|j|codes[*j]>=1000).unwrap_or(i);
  let mut values:Vec<(usize,f64)>=Vec::new();
  let mut add:Vec<(u16,f64)>=Vec::new();
  let mut single=|code:u16,value:f64,allow_insert:bool|->R<()>{
   let matches:Vec<_>=(start..stop).filter(|j|codes[*j]==code).collect();
   match matches.as_slice(){[j]=>values.push((*j,finite(value)?)),[] if allow_insert=>add.push((code,finite(value)?)),_=>return Err("INVALID_DXF")}
   Ok(())
  };
  match *operation{
   CadEdit::Translate{dx,dy}=>{
    if finite(dx).is_err()||finite(dy).is_err()||(dx==0.&&dy==0.){return Err("INVALID_TRANSLATION")}
    let mut boundary=false;let mut seeds=false;
    for j in start..stop{
     let c=codes[j];
     if kind=="HATCH"{match c{92=>{boundary=true;seeds=false},97=>boundary=false,98=>seeds=true,_=>{}}}
     let delta=match (kind,c){
      ("LINE",10|11)=>dx,("LINE",20|21)=>dy,
      ("POINT"|"LWPOLYLINE"|"ARC"|"CIRCLE"|"INSERT",10)=>dx,
      ("POINT"|"LWPOLYLINE"|"ARC"|"CIRCLE"|"INSERT",20)=>dy,
      ("HATCH",10) if boundary||seeds=>dx,("HATCH",20) if boundary||seeds=>dy,
      _=>continue
     };
     values.push((j,finite(num(field(j))?+delta)?));
    }
   },
   CadEdit::Radius{radius} if matches!(kind,"ARC"|"CIRCLE")=>{
    if radius<=0.{return Err("INVALID_RADIUS")}single(40,radius,false)?;
   },
   CadEdit::ArcAngles{start:angle_start,end:angle_end} if kind=="ARC"=>{
    if !(0. ..=360.).contains(&angle_start)||!(0. ..=360.).contains(&angle_end)||(angle_end-angle_start).rem_euclid(360.)<=1e-10{return Err("INVALID_ARC")}
    single(50,angle_start,false)?;single(51,angle_end,false)?;
   },
   CadEdit::BlockTransform{scale_x,scale_y,rotation} if kind=="INSERT"=>{
    if scale_x==0.||scale_y==0.||!(-360. ..=360.).contains(&rotation){return Err("INVALID_BLOCK_TRANSFORM")}
    single(41,scale_x,true)?;single(42,scale_y,true)?;single(50,rotation,true)?;
   },
   CadEdit::Vertex{path,vertex,x,y} if matches!(kind,"LWPOLYLINE"|"HATCH")=>{
    let(a,b)=if kind=="LWPOLYLINE"{
     if path!=0{return Err("INVALID_PATH")} (start,stop)
    }else{
     // Boundary edits for the explicitly supported non-associative solid
     // hatch profile. Refuse seed-bearing hatches rather than invalidate seeds.
     if (start..stop).any(|j|codes[j]==98&&num(field(j)).ok()!=Some(0.)){return Err("HATCH_SEED_EDIT_UNSUPPORTED")}
     let begins:Vec<_>=(start..stop).filter(|j|codes[*j]==92).collect();
     let a=*begins.get(path).ok_or("INVALID_PATH")?;
     let stop=begins.get(path+1).copied().unwrap_or(stop);
     let b=(a..stop).find(|j|codes[*j]==97).ok_or("INVALID_HATCH")?;
     (a,b)
    };
    let xs:Vec<_>=(a..b).filter(|j|codes[*j]==10).collect();
    let ys:Vec<_>=(a..b).filter(|j|codes[*j]==20).collect();
    if xs.len()!=ys.len(){return Err("INVALID_POLYLINE")}
    values.push((*xs.get(vertex).ok_or("INVALID_VERTEX")?,finite(x)?));
    values.push((*ys.get(vertex).ok_or("INVALID_VERTEX")?,finite(y)?));
   },
   _=>return Err("CAD_EDIT_TYPE_UNSUPPORTED")
  }
  for(j,v)in values{
   let at=j*2+1;let end=if lines[at].ends_with("\r\n"){"\r\n"}else if lines[at].ends_with('\n'){"\n"}else{""};
   if edits.insert(at,format!("{v}{end}")).is_some(){return Err("INVALID_DXF")}
  }
  if !add.is_empty(){
   let end=if lines[start*2].ends_with("\r\n"){"\r\n"}else{"\n"};
   insertion=Some((stop*2,add.into_iter().map(|(c,v)|format!("{c}{end}{v}{end}")).collect()));
  }
 }
 if !found||(edits.is_empty()&&insertion.is_none()){return Err("STALE_ENTITY")}
 let mut result=String::with_capacity(raw.len());
 for(i,line)in lines.iter().enumerate(){
  if let Some((at,data))=&insertion{if *at==i{result.push_str(data)}}
  result.push_str(edits.get(&i).map(String::as_str).unwrap_or(line))
 }
 if result.len()>4*1024*1024{return Err("RESOURCE_LIMIT")}
 std::fs::write(out,result).map_err(|_|"FILE_UNAVAILABLE")?;
 let after=open(out)?;
 if after.len()!=before.len(){return Err("CAD_FIDELITY_FAILED")}
 for(a,b)in before.iter().zip(&after){
  if a.handle!=handle{if a!=b{return Err("CAD_FIDELITY_FAILED")}continue}
  if a.handle!=b.handle||a.layer!=b.layer||a.kind!=b.kind||a.fill!=b.fill||a.paths.len()!=b.paths.len(){return Err("CAD_FIDELITY_FAILED")}
  if let CadEdit::Translate{dx,dy}=*operation{
   for(p,q)in a.paths.iter().zip(&b.paths){
    if p.len()!=q.len(){return Err("CAD_FIDELITY_FAILED")}
    for(x,y)in p.iter().zip(q){for k in 0..2{let expected=x[k]+if k==0{dx}else{dy};if (expected-y[k]).abs()>1e-7+expected.abs()*1e-12{return Err("CAD_FIDELITY_FAILED")}}}
   }
  }
 }
 Ok(())
}
#[cfg(test)]mod tests{
 use super::*;
 #[test]fn translate_all_extended_types_preserves_records(){
  let dir=tempfile::tempdir().unwrap();let src=dir.path().join("input.dxf");let dst=dir.path().join("output.dxf");
  let block="0\nSECTION\n2\nBLOCKS\n0\nBLOCK\n2\nB\n10\n0\n20\n0\n0\nPOINT\n10\n1\n20\n2\n0\nENDBLK\n0\nENDSEC\n";
  let items=[
   ("LWPOLYLINE","90\n2\n70\n1\n10\n1\n20\n2\n10\n3\n20\n4\n"),
   ("ARC","10\n1\n20\n2\n40\n2\n50\n0\n51\n90\n"),
   ("CIRCLE","10\n1\n20\n2\n40\n2\n"),
   ("HATCH","10\n0\n20\n0\n70\n1\n91\n1\n92\n2\n72\n0\n73\n1\n93\n3\n10\n0\n20\n0\n10\n1\n20\n0\n10\n0\n20\n1\n97\n0\n98\n1\n10\n0.2\n20\n0.2\n"),
   ("INSERT","2\nB\n10\n1\n20\n2\n50\n90\n")
  ];
  for(kind,fields)in items{
   let input=format!("{block}0\nSECTION\n2\nENTITIES\n0\n{kind}\n5\nAB\n8\nSurvey\n62\n4\n{fields}1001\nTEST\n1010\n123\n0\nPOINT\n5\nCD\n8\nOther\n10\n7\n20\n8\n0\nENDSEC\n0\nEOF\n");
   std::fs::write(&src,&input).unwrap();translate(&src,&dst,"AB",10.,-2.).unwrap();
   let out=std::fs::read_to_string(&dst).unwrap();assert!(out.starts_with(block));assert!(out.contains("62\n4\n"));assert!(out.contains("1001\nTEST\n1010\n123\n"));assert_eq!(std::fs::read_to_string(&src).unwrap(),input);
   assert_eq!(translate(&src,&dst,"AB",f64::NAN,0.),Err("INVALID_TRANSLATION"));
   assert_eq!(translate(&src,&dst,"FF",1.,0.),Err("STALE_ENTITY"));
  }
 }
 #[test]fn native_property_edits_preserve_exact_entity_types(){
  let dir=tempfile::tempdir().unwrap();let src=dir.path().join("input.dxf");let dst=dir.path().join("output.dxf");
  let block="0\nSECTION\n2\nBLOCKS\n0\nBLOCK\n2\nB\n10\n0\n20\n0\n0\nPOINT\n10\n1\n20\n0\n0\nENDBLK\n0\nENDSEC\n";
  let fixture=|kind:&str,fields:&str|format!("{block}0\nSECTION\n2\nENTITIES\n0\n{kind}\n5\nAB\n8\nSurvey\n{fields}0\nENDSEC\n0\nEOF\n");
  std::fs::write(&src,fixture("CIRCLE","10\n0\n20\n0\n40\n2\n")).unwrap();
  edit(&src,&dst,"AB",&CadEdit::Radius{radius:5.}).unwrap();assert_eq!(open(&dst).unwrap()[0].paths[0][0],[5.,0.]);
  assert_eq!(edit(&src,&dst,"AB",&CadEdit::Radius{radius:-1.}),Err("INVALID_RADIUS"));
  std::fs::write(&src,fixture("ARC","10\n0\n20\n0\n40\n2\n50\n0\n51\n90\n")).unwrap();
  edit(&src,&dst,"AB",&CadEdit::ArcAngles{start:90.,end:180.}).unwrap();let paths=open(&dst).unwrap();assert!((paths[0].paths[0][0][1]-2.).abs()<1e-12);
  std::fs::write(&src,fixture("LWPOLYLINE","90\n2\n70\n1\n10\n1\n20\n2\n10\n3\n20\n4\n")).unwrap();
  edit(&src,&dst,"AB",&CadEdit::Vertex{path:0,vertex:0,x:7.,y:8.}).unwrap();assert_eq!(open(&dst).unwrap()[0].paths[0],vec![[7.,8.],[3.,4.],[7.,8.]]);
  std::fs::write(&src,fixture("HATCH","10\n0\n20\n0\n70\n1\n91\n1\n92\n2\n72\n0\n73\n1\n93\n3\n10\n0\n20\n0\n10\n1\n20\n0\n10\n0\n20\n1\n97\n0\n98\n0\n")).unwrap();
  edit(&src,&dst,"AB",&CadEdit::Vertex{path:0,vertex:1,x:3.,y:0.}).unwrap();assert_eq!(open(&dst).unwrap()[0].paths[0][1],[3.,0.]);
  std::fs::write(&src,fixture("INSERT","2\nB\n10\n10\n20\n20\n")).unwrap();
  edit(&src,&dst,"AB",&CadEdit::BlockTransform{scale_x:2.,scale_y:3.,rotation:90.}).unwrap();let paths=open(&dst).unwrap();assert!((paths[0].paths[0][0][0]-10.).abs()<1e-12);assert!((paths[0].paths[0][0][1]-22.).abs()<1e-12);
 }
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
