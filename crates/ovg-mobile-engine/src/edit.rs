//! Standalone editing profile: XY WGS84 point, line, polygon and multi variants.
//! Drafts are values only; a commit must pass the same validation again.
use crate::Error;
use geo::{Geometry,Coord,LineString,algorithm::Validation};
use ovg_spatial::{parse_xy_wkb,encode_wkb,validate_geometry,ValidationLimits};
use serde::{Serialize,Deserialize};
pub const MAX_EDIT_VERTICES:usize=4096;
fn limits()->ValidationLimits{ValidationLimits{max_vertices:MAX_EDIT_VERTICES,max_rings_per_polygon:128,max_nesting_depth:4}}
fn valid_xy(x:f64,y:f64)->bool{x.is_finite()&&y.is_finite()&&x.abs()<=180.&&y.abs()<=90.}
pub fn validate(wkb:&[u8])->Result<Geometry<f64>,Error>{
    if wkb.len()>crate::MAX_BYTES{return Err(Error::Limit);}
    let g=parse_xy_wkb(wkb,&limits()).map_err(|_|Error::InvalidBatch)?.geometry;
    if !matches!(g,Geometry::Point(_)|Geometry::LineString(_)|Geometry::Polygon(_)|Geometry::MultiPoint(_)|Geometry::MultiLineString(_)|Geometry::MultiPolygon(_)){return Err(Error::InvalidBatch);}
    let r=validate_geometry(&g,&limits()).map_err(|_|Error::InvalidBatch)?;
    if !valid_xy(r.bbox.min_x,r.bbox.min_y)||!valid_xy(r.bbox.max_x,r.bbox.max_y){return Err(Error::InvalidBatch);}
    // Regional planar topology only; reject antimeridian and polar ambiguity.
    if r.bbox.max_x-r.bbox.min_x>=180.||r.bbox.min_y<=-85.||r.bbox.max_y>=85.{return Err(Error::Spatial("UNSUPPORTED_TOPOLOGY_EXTENT".into()));}
    g.check_validation().map_err(|e|Error::Spatial(e.to_string()))?;
    Ok(g)
}
pub fn draft(kind:&str,xy:&[f64])->Result<Vec<u8>,Error>{
    if xy.len()%2!=0||xy.len()/2>MAX_EDIT_VERTICES-1||xy.chunks_exact(2).any(|p|!valid_xy(p[0],p[1])){return Err(Error::InvalidBatch);}
    let mut c:Vec<Coord<f64>>=xy.chunks_exact(2).map(|p|Coord{x:p[0],y:p[1]}).collect();
    let g=match kind{
        "point" if c.len()==1=>Geometry::Point(geo::Point(c[0])),
        "line" if c.len()>=2=>Geometry::LineString(LineString(c)),
        "polygon" if c.len()>=3=>{if c.first()!=c.last(){c.push(c[0]);}Geometry::Polygon(geo::Polygon::new(LineString(c),vec![]))},
        _=>return Err(Error::InvalidBatch)
    };
    let bytes=encode_wkb(&g,None).map_err(|_|Error::InvalidBatch)?;
    validate(&bytes)?;Ok(bytes)
}
/// Split at an existing interior vertex, preserving full-resolution coordinates.
pub fn split_line(wkb:&[u8],index:usize)->Result<(Vec<u8>,Vec<u8>),Error>{
    let Geometry::LineString(line)=validate(wkb)? else{return Err(Error::InvalidBatch)};
    if index==0||index>=line.0.len()-1{return Err(Error::InvalidBatch);}
    let make=|c:&[Coord<f64>]|{let b=encode_wkb(&Geometry::LineString(LineString(c.to_vec())),None).map_err(|_|Error::InvalidBatch)?;validate(&b)?;Ok(b)};
    Ok((make(&line.0[..=index])?,make(&line.0[index..])?))
}
/// Join lines only at an exactly coincident endpoint; no implicit coordinate move.
pub fn merge_lines(a:&[u8],b:&[u8])->Result<Vec<u8>,Error>{
    let Geometry::LineString(mut a)=validate(a)? else{return Err(Error::InvalidBatch)};
    let Geometry::LineString(mut b)=validate(b)? else{return Err(Error::InvalidBatch)};
    if a.0.last()!=b.0.first(){
        if a.0.last()==b.0.last(){b.0.reverse();}
        else if a.0.first()==b.0.first(){a.0.reverse();}
        else if a.0.first()==b.0.last(){std::mem::swap(&mut a,&mut b);}
        else{return Err(Error::InvalidBatch);}
    }
    a.0.extend_from_slice(&b.0[1..]);
    let out=encode_wkb(&Geometry::LineString(a),None).map_err(|_|Error::InvalidBatch)?;validate(&out)?;Ok(out)
}
#[derive(Debug,Clone,Copy,Serialize,Deserialize,PartialEq)]
pub struct Snap{pub x:f64,pub y:f64,pub distance_m:f64,pub vertex:bool}
/// Local tangent metric for candidate ranking within <=100m, no survey accuracy
/// claim. Authoritative coordinate is the exact vertex or interpolated XY edge.
pub fn snap(wkb:&[u8],x:f64,y:f64,tolerance_m:f64)->Result<Option<Snap>,Error>{
    if !valid_xy(x,y)||y.abs()>85.||!tolerance_m.is_finite()||tolerance_m<=0.||tolerance_m>100.{return Err(Error::InvalidBatch);}
    let g=parse_xy_wkb(wkb,&ValidationLimits::default()).map_err(|_|Error::InvalidBatch)?.geometry;
    use geo::{CoordsIter,LinesIter};
    let scale_x=111319.49079327357*y.to_radians().cos();let scale_y=111132.0;
    let mut best:Option<Snap>=None;
    let mut consider=|c:Coord<f64>,vertex:bool|{
        let d=((c.x-x)*scale_x).hypot((c.y-y)*scale_y);
        if d<=tolerance_m&&best.is_none_or(|b|d<b.distance_m-1e-9||((d-b.distance_m).abs()<=1e-9&&vertex&&!b.vertex)){best=Some(Snap{x:c.x,y:c.y,distance_m:d,vertex});}
    };
    for c in g.coords_iter(){consider(c,true);}
    for line in g.lines_iter(){
        if (line.end.x-line.start.x).abs()>=180.{continue;}
        let ax=(line.start.x-x)*scale_x;let ay=(line.start.y-y)*scale_y;
        let dx=(line.end.x-line.start.x)*scale_x;let dy=(line.end.y-line.start.y)*scale_y;
        let denom=dx*dx+dy*dy;if denom==0.{continue;}
        let t=(-(ax*dx+ay*dy)/denom).clamp(0.,1.);
        if t>0.&&t<1.{consider(Coord{x:line.start.x+t*(line.end.x-line.start.x),y:line.start.y+t*(line.end.y-line.start.y)},false);}
    }
    Ok(best)
}
