//! Explicit WGS84 ellipsoidal or registered native planar measurement.
use crate::Error;
use geographiclib_rs::{Geodesic,InverseGeodesic,PolygonArea,Winding};
use ovg_core::crs::{lookup_readable_crs,CrsUnit,NATIVE_VN2000_ZONES};
use serde::{Serialize,Deserialize};
#[derive(Debug,Clone,Copy,PartialEq,Serialize,Deserialize)]
pub struct Segment {pub metres:f64,pub bearing_deg:Option<f64>}
fn check_xy(xy:&[f64],epsg:u32,min:usize)->Result<(),Error>{
    if xy.len()%2!=0||xy.len()/2<min||xy.len()/2>4096||xy.iter().any(|x|!x.is_finite()){return Err(Error::InvalidBatch);}
    if epsg==4326 {
        if xy.chunks_exact(2).any(|p|p[0].abs()>180.||p[1].abs()>90.){return Err(Error::InvalidBatch);}
    } else {
        // Web Mercator grid metres are not ground metres. Exclude by default.
        if epsg==3857{return Err(Error::Spatial("DISTORTED_CRS_USE_GEODESIC".into()));}
        if let Some(p)=lookup_readable_crs(epsg){
            if p.unit!=CrsUnit::Metre{return Err(Error::Spatial("PROJECTED_METRE_CRS_REQUIRED".into()));}
            if let Some(b)=p.native_bounds{
                if xy.chunks_exact(2).any(|v|!b.contains(v[0],v[1],0.)){return Err(Error::InvalidBatch);}
            }
        }else if !NATIVE_VN2000_ZONES.iter().any(|z|z.code==epsg){
            return Err(Error::Spatial("UNSUPPORTED_CRS".into()));
        } else {
            // Extended zones have geographic area bounds, not a proven native
            // coordinate envelope here. Do not accept arbitrary planar data.
            return Err(Error::Spatial("UNQUALIFIED_NATIVE_MEASURE_PROFILE".into()));
        }
    }
    Ok(())
}
pub fn segments(epsg:u32,xy:&[f64])->Result<Vec<Segment>,Error>{
    check_xy(xy,epsg,2)?;
    let g=Geodesic::wgs84();
    let mut out=Vec::with_capacity(xy.len()/2-1);
    for i in (0..xy.len()-2).step_by(2){
        let (x1,y1,x2,y2)=(xy[i],xy[i+1],xy[i+2],xy[i+3]);
        let (d,az)=if epsg==4326{
            let (d,az,_,_):(f64,f64,f64,f64)=g.inverse(y1,x1,y2,x2);(d,az)
        }else{((x2-x1).hypot(y2-y1),(x2-x1).atan2(y2-y1).to_degrees())};
        if !d.is_finite() || !az.is_finite(){return Err(Error::InvalidBatch);}
        out.push(Segment{metres:d,bearing_deg:if d==0.{None}else{Some(az.rem_euclid(360.))}});
    }
    Ok(out)
}
#[derive(Debug,Clone,Copy,PartialEq,Serialize,Deserialize)]
pub struct RingMeasure{pub perimeter_m:f64,pub area_m2:f64}
/// Simple closed ring only; callers must validate topology before claiming parcel area.
pub fn ring(epsg:u32,xy:&[f64])->Result<RingMeasure,Error>{
    check_xy(xy,epsg,4)?;
    if xy[..2]!=xy[xy.len()-2..]{return Err(Error::InvalidBatch);}
    let (perimeter,area)=if epsg==4326{
        let g=Geodesic::wgs84();let mut p=PolygonArea::new(&g,Winding::CounterClockwise);
        for c in xy[..xy.len()-2].chunks_exact(2){p.add_point(c[1],c[0]);}
        let (p,a,_)=p.compute(true);(p,a.abs())
    }else{
        let p=segments(epsg,xy)?.iter().map(|s|s.metres).sum();
        let (ox,oy)=(xy[0],xy[1]);let mut a=0.;
        for i in (0..xy.len()-2).step_by(2){a+=(xy[i]-ox)*(xy[i+3]-oy)-(xy[i+2]-ox)*(xy[i+1]-oy);}
        (p,(a*0.5_f64).abs())
    };
    if !perimeter.is_finite()||!area.is_finite()||area==0.{return Err(Error::InvalidBatch);}
    Ok(RingMeasure{perimeter_m:perimeter,area_m2:area})
}
