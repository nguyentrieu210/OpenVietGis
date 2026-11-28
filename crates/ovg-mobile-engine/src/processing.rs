//! Bounded mobile adapter of shared R008 registry. No server dependencies.
//! Overlay profile: valid regional XY polygon input, explicit source CRS.
//! Buffer uses projected metric coordinates only; no degree-based distance.
use crate::{Error,AlgorithmId};
use geo::{Geometry,MultiPolygon,BooleanOps,Buffer,Intersects,Relate,MapCoords,CoordsIter,algorithm::Validation};
use ovg_spatial::{parse_xy_wkb,encode_wkb,ValidationLimits,validate_geometry};
use serde::{Serialize,Deserialize};
use serde_json::{Value,json};
use std::collections::{BTreeMap,HashSet};
const ROWS:usize=128;
const VERTICES:usize=16384;
#[derive(Clone,Debug,Serialize,Deserialize)]
pub struct Input{pub id:String,pub wkb:Vec<u8>,pub attributes:Value}
#[derive(Debug,Serialize,Deserialize)]
pub struct Output{pub wkb:Vec<u8>,pub attributes:Value,pub source_ids:Vec<String>,pub overlay_ids:Vec<String>}
#[derive(Debug,Serialize,Deserialize)]
pub struct ResultSet{pub epsg:u32,pub algorithm:AlgorithmId,pub parameters:Value,pub rows:Vec<Output>,pub dropped_empty:usize,pub adapter:String}
fn fail()->Error{Error::Spatial("UNSUPPORTED_LOCAL_PROCESSING_PROFILE".into())}
fn check(g:&Geometry<f64>)->Result<(),Error>{
 if g.coords_count()>4096||g.coords_iter().any(|c|!c.x.is_finite()||!c.y.is_finite()){return Err(Error::Limit);}
 validate_geometry(g,&ValidationLimits::default()).map_err(|_|Error::InvalidBatch)?;
 g.check_validation().map_err(|e|Error::Spatial(e.to_string()))?;Ok(())
}
fn decode(rows:&[Input],epsg:u32)->Result<Vec<Geometry<f64>>,Error>{
 if rows.len()>ROWS{return Err(Error::Limit);}
 let mut ids=HashSet::new();let mut total=0;let mut bytes=0;
 rows.iter().map(|r|{
  if r.id.is_empty()||r.id.len()>128||!ids.insert(&r.id)||!r.attributes.is_object(){return Err(Error::InvalidBatch);}
  bytes+=r.wkb.len()+r.attributes.to_string().len();if bytes>crate::MAX_BYTES{return Err(Error::Limit);}
  let p=parse_xy_wkb(&r.wkb,&ValidationLimits::default()).map_err(|_|Error::InvalidBatch)?;
  if p.srid.is_some_and(|s|s!=epsg){return Err(Error::InvalidBatch);}
  let g=p.geometry;check(&g)?;
  if epsg==4326{
   let b=validate_geometry(&g,&ValidationLimits::default()).map_err(|_|Error::InvalidBatch)?.bbox;
   if b.min_x< -180.||b.max_x>180.||b.min_y<=-85.||b.max_y>=85.||b.max_x-b.min_x>=180.{return Err(fail());}
  }
  total+=g.coords_count();if total>VERTICES{return Err(Error::Limit);}Ok(g)
 }).collect()
}
fn poly(g:&Geometry<f64>)->Result<MultiPolygon<f64>,Error>{match g{Geometry::Polygon(p)=>Ok(MultiPolygon(vec![p.clone()])),Geometry::MultiPolygon(p)=>Ok(p.clone()),_=>Err(fail())}}
fn combined(gs:&[Geometry<f64>],cancel:&impl Fn()->bool)->Result<MultiPolygon<f64>,Error>{
 let mut all=MultiPolygon(vec![]);for g in gs{if cancel(){return Err(Error::Cancelled);}all=all.union(&poly(g)?);if all.coords_count()>VERTICES{return Err(Error::Limit);}}Ok(all)
}
pub fn run(algorithm:AlgorithmId,epsg:u32,input:&[Input],overlay:&[Input],parameters:&Value,cancel:impl Fn()->bool,mut progress:impl FnMut(u32))->Result<ResultSet,Error>{
 let params=ovg_processing::registry::validate_algorithm_parameters(algorithm,parameters.as_object().ok_or(Error::InvalidBatch)?).map_err(|e|Error::Spatial(e.to_string()))?;
 if !matches!(epsg,4326|3857){return Err(fail());}
 if input.is_empty()||cancel(){return Err(if cancel(){Error::Cancelled}else{Error::InvalidBatch});}
 let a=decode(input,epsg)?;let b=decode(overlay,epsg)?;
 let needs=matches!(algorithm,AlgorithmId::Clip|AlgorithmId::Difference|AlgorithmId::Intersection|AlgorithmId::Union|AlgorithmId::SpatialJoin);
 if needs==overlay.is_empty(){return Err(Error::InvalidBatch);}
 let target=if algorithm==AlgorithmId::Reproject{params["target_srid"].as_u64().ok_or(Error::InvalidBatch)? as u32}else{epsg};
 let mut rows=Vec::new();let mut dropped=0usize;let mut size=0usize;
 let mut emit=|g:Geometry<f64>,attributes:Value,source_ids:Vec<String>,overlay_ids:Vec<String>|->Result<(),Error>{
  if cancel(){return Err(Error::Cancelled);}
  if matches!(&g,Geometry::MultiPolygon(p) if p.0.is_empty()){dropped+=1;return Ok(());}
  check(&g)?;let wkb=encode_wkb(&g,None).map_err(|_|Error::InvalidBatch)?;
  size+=wkb.len()+attributes.to_string().len()+source_ids.iter().map(String::len).sum::<usize>()+overlay_ids.iter().map(String::len).sum::<usize>();
  if size>crate::MAX_BYTES||rows.len()>=4096{return Err(Error::Limit);}
  rows.push(Output{wkb,attributes,source_ids,overlay_ids});Ok(())
 };
 let ids=|r:&Input|vec![r.id.clone()];
 match algorithm{
  AlgorithmId::Dissolve=>{
   let field=params["group_by_field"].as_str();let mut groups:BTreeMap<String,Vec<usize>>=BTreeMap::new();
   for(i,r)in input.iter().enumerate(){let key=if let Some(f)=field{r.attributes.get(f).ok_or(Error::InvalidBatch)?.to_string()}else{"null".into()};groups.entry(key).or_default().push(i);}
   for(key,indices)in groups{let gs=indices.iter().map(|&i|a[i].clone()).collect::<Vec<_>>();
    let attributes=if let Some(f)=field{let mut m=serde_json::Map::new();m.insert(f.into(),serde_json::from_str(&key).map_err(|_|Error::InvalidBatch)?);Value::Object(m)}else{json!({})};
    emit(Geometry::MultiPolygon(combined(&gs,&cancel)?),attributes,indices.iter().map(|&i|input[i].id.clone()).collect(),vec![])?;
   }
  },
  AlgorithmId::Union=>{
   // Preserve both sides of polygon overlay. Reject internal overlaps rather
   // than produce duplicate ambiguous pieces in this bounded profile.
   for gs in [&a,&b]{for i in 0..gs.len(){for j in i+1..gs.len(){if cancel(){return Err(Error::Cancelled);}if !poly(&gs[i])?.intersection(&poly(&gs[j])?).0.is_empty(){return Err(fail());}}}}
   let ua=combined(&a,&cancel)?;let ub=combined(&b,&cancel)?;
   for(i,g)in a.iter().enumerate(){let p=poly(g)?;
    emit(Geometry::MultiPolygon(p.difference(&ub)),json!({"input":input[i].attributes,"overlay":null}),ids(&input[i]),vec![])?;
    for(j,h)in b.iter().enumerate(){if cancel(){return Err(Error::Cancelled);}if g.intersects(h){emit(Geometry::MultiPolygon(p.intersection(&poly(h)?)),json!({"input":input[i].attributes,"overlay":overlay[j].attributes}),ids(&input[i]),ids(&overlay[j]))?;}}
   }
   for(j,h)in b.iter().enumerate(){emit(Geometry::MultiPolygon(poly(h)?.difference(&ua)),json!({"input":null,"overlay":overlay[j].attributes}),vec![],ids(&overlay[j]))?;}
  },
  _=>{
   let mask=if matches!(algorithm,AlgorithmId::Clip|AlgorithmId::Difference){Some(combined(&b,&cancel)?)}else{None};
   for(i,g)in a.iter().enumerate(){
    if cancel(){return Err(Error::Cancelled);}
    let r=&input[i];
    match algorithm{
     AlgorithmId::Buffer=>{
      // Geo buffer default round style only. Registry quad_segs=8 is not
      // silently claimed equivalent to GEOS. Explicit alternative rejected.
      if epsg!=3857||parameters.get("quad_segs").is_some(){return Err(fail());}
      let d=params["distance_meters"].as_f64().ok_or(Error::InvalidBatch)?;
      if d>10000.{return Err(fail());}
      emit(Geometry::MultiPolygon(g.buffer(d)),r.attributes.clone(),ids(r),vec![])?;
     },
     AlgorithmId::Clip|AlgorithmId::Difference=>{
      let p=poly(g)?;let mask=mask.as_ref().ok_or(Error::InvalidBatch)?;
      let out=if algorithm==AlgorithmId::Clip{p.intersection(mask)}else{p.difference(mask)};
      emit(Geometry::MultiPolygon(out),r.attributes.clone(),ids(r),overlay.iter().map(|r|r.id.clone()).collect())?;
     },
     AlgorithmId::Intersection=>for(j,h)in b.iter().enumerate(){if cancel(){return Err(Error::Cancelled);}if g.intersects(h){emit(Geometry::MultiPolygon(poly(g)?.intersection(&poly(h)?)),r.attributes.clone(),ids(r),ids(&overlay[j]))?;}},
     AlgorithmId::Reproject=>{
      if !matches!(target,4326|3857){return Err(fail());}
      let mut error=None;
      let mapped=g.map_coords(|c|match crate::crs::transform(epsg,target,c.x,c.y){Ok([x,y])=>geo::Coord{x,y},Err(e)=>{error=Some(e);c}});
      if let Some(e)=error{return Err(e);}emit(mapped,r.attributes.clone(),ids(r),vec![])?;
     },
     AlgorithmId::SpatialJoin=>{
      let mut found=false;for(j,h)in b.iter().enumerate(){if cancel(){return Err(Error::Cancelled);}
       let relation=g.relate(h);let yes=match params["predicate"].as_str().unwrap_or(""){
        "intersects"=>relation.is_intersects(),"within"=>relation.is_within(),"contains"=>relation.is_contains(),"touches"=>relation.is_touches(),"overlaps"=>relation.is_overlaps(),"crosses"=>relation.is_crosses(),"equals"=>relation.is_equal_topo(),_=>return Err(fail())};
       if yes{found=true;emit(g.clone(),json!({"input":r.attributes,"overlay":overlay[j].attributes}),ids(r),ids(&overlay[j]))?;if params["match_mode"]=="first"{break;}}
      }if !found&&params["join_type"]=="left"{emit(g.clone(),json!({"input":r.attributes,"overlay":null}),ids(r),vec![])?;}
     },
     _=>return Err(fail()),
    }
    progress(((i+1)*99/a.len())as u32);
   }
  }
 }
 if cancel(){return Err(Error::Cancelled);}progress(100);
 Ok(ResultSet{epsg:target,algorithm,parameters:params,rows,dropped_empty:dropped,adapter:"geo-0.31-mobile-bounded-v1".into()})
}
