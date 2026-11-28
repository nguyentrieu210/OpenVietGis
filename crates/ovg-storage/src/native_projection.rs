//! Same-datum whole-layer projection, not a national datum operation.
use ovg_core::{AppError,crs::{is_extended_native_vn2000,is_native_vn2000,NativeVn2000Zone,NATIVE_VN2000_ZONES}};
use serde_json::{json,Value};
use sqlx::{Postgres,Transaction};
use uuid::Uuid;
pub(crate) fn is_native_pair(a:u32,b:u32)->bool{
    is_native_vn2000(a)&&is_native_vn2000(b)&&(is_extended_native_vn2000(a)||is_extended_native_vn2000(b))
}
fn zone(code:u32)->Option<&'static NativeVn2000Zone>{NATIVE_VN2000_ZONES.iter().find(|z|z.code==code)}
fn pipeline(z:&NativeVn2000Zone)->String{
    format!("+proj=pipeline +step +proj=unitconvert +xy_in=deg +xy_out=rad +step +proj=tmerc +lat_0=0 +lon_0={} +k={} +x_0=500000 +y_0=0 +ellps=WGS84",z.central_meridian,z.scale_factor)
}
fn bounds(z:Option<&NativeVn2000Zone>)->[f64;4]{
    let mut b=z.map_or([102.14,8.33,109.53,23.4],|z|z.bounds);
    if let Some(z)=z{let half=if z.scale_factor==0.9996{3.0}else{1.5};b[0]=b[0].max(z.central_meridian-half);b[2]=b[2].min(z.central_meridian+half);}
    b
}
pub(crate) struct NativeProjection{source:u32,target:u32,src:Option<&'static NativeVn2000Zone>,dst:Option<&'static NativeVn2000Zone>}
impl NativeProjection{
    pub(crate) fn new(source:u32,target:u32)->Result<Self,AppError>{
        if !is_native_pair(source,target){return Err(AppError::unsupported_transformation(source,target));}
        Ok(Self{source,target,src:zone(source),dst:zone(target)})
    }
    fn geographic(&self,geometry:&str)->String{
        self.src.map_or_else(||geometry.to_owned(),|z|format!("ST_InverseTransformPipeline({geometry}, '{}',4756)",pipeline(z)))
    }
    pub(crate) fn expression(&self,geometry:&str)->String{
        if self.source==self.target{return geometry.to_owned();}
        let geo=self.geographic(geometry);
        self.dst.map_or_else(||geo.clone(),|z|format!("ST_TransformPipeline({geo}, '{}',{})",pipeline(z),z.code))
    }
    pub(crate) fn wire(&self)->Value{
        json!({"source_code":self.source,"target_code":self.target,"operation_id":"native-vn2000-projection","method":"fixed same-datum Transverse Mercator pipeline","validated_area":"Intersection of declared EPSG geographic rectangles and zone widths; not a datum conversion","tolerance":0.02,"tolerance_unit":"metre","evidence_id":"native-vn2000-proj-mathematical-controls"})
    }
    pub(crate) async fn verify_source(&self,tx:&mut Transaction<'_,Postgres>,org:Uuid,project:Uuid,source:Uuid)->Result<(),AppError>{
        // Fixed registry parameters only. Domain tests cover all input geometry,
        // not merely its centroid or the first page of features.
        let a=bounds(self.src);let b=bounds(self.dst);
        let w=a[0].max(b[0]);let s=a[1].max(b[1]);let e=a[2].min(b[2]);let n=a[3].min(b[3]);
        let outside=||AppError::transformation_out_of_area("VN-2000 layer is outside the intersection of the source and target projection zones.");
        if w>e||s>n{return Err(outside());}
        let stats: (i64,i64,Option<i32>)=sqlx::query_as("SELECT count(*)::bigint,COALESCE(sum(ST_NPoints(f.geometry)),0)::bigint,(SELECT feature_count FROM spatial.data_sources s WHERE s.organization_id=$1 AND s.project_id=$2 AND s.id=$3 AND s.validation_state='ready' AND s.lifecycle='active' AND s.stored_crs=$4) FROM spatial.reference_features f WHERE f.organization_id=$1 AND f.project_id=$2 AND f.source_id=$3")
            .bind(org).bind(project).bind(source).bind(self.source as i32).fetch_one(&mut **tx).await.map_err(crate::catalog::db_error)?;
        if stats.2.map(i64::from)!=Some(stats.0)||stats.0>crate::processing_jobs::MAX_PROCESSING_FEATURES||stats.1>crate::processing_jobs::MAX_PROCESSING_POSITIONS{
            return Err(AppError::validation("PROCESSING_LIMIT","Native projection input exceeds bounds or source count disagrees with stored rows"));
        }
        let geographic=self.geographic("f.geometry");
        let output=self.expression("f.geometry");
        let src_domain=if self.src.is_some(){"ST_CoveredBy(f.geometry,ST_MakeEnvelope(100000,0,900000,3000000,ST_SRID(f.geometry)))"}else{"true"};
        let dst_domain=if self.dst.is_some(){format!("ST_CoveredBy({output},ST_MakeEnvelope(100000,0,900000,3000000,{}))",self.target)}else{"true".into()};
        let sql=format!("SELECT NOT EXISTS(SELECT 1 FROM spatial.reference_features f WHERE organization_id=$1 AND project_id=$2 AND source_id=$3 AND
          (f.geometry_state='present' AND f.geometry IS NOT NULL AND ST_SRID(f.geometry)={} AND ST_Zmflag(f.geometry)=0 AND ST_IsValid(f.geometry) AND NOT ST_IsEmpty(f.geometry)
          AND CASE WHEN {src_domain} THEN ST_CoveredBy({geographic},ST_MakeEnvelope({w},{s},{e},{n},4756)) AND ST_IsValid({output}) AND NOT ST_IsEmpty({output}) AND {dst_domain} ELSE false END) IS NOT TRUE)",self.source);
        let valid:bool=sqlx::query_scalar(&sql).bind(org).bind(project).bind(source).fetch_one(&mut **tx).await.map_err(crate::catalog::db_error)?;
        if !valid{return Err(outside());}Ok(())
    }
}
#[cfg(test)]
mod tests{
    use super::*;
    #[test]fn fixed_native_pipeline_and_no_cross_datum(){
        assert!(!is_native_pair(4326,9212));assert!(!is_native_pair(5899,4756));assert!(is_native_pair(9212,3405));
        let p=NativeProjection::new(9212,3405).unwrap();let sql=p.expression("f.geometry");
        assert!(sql.contains("ST_InverseTransformPipeline"));assert!(sql.contains("+lon_0=106.25"));assert!(sql.contains("+lon_0=105"));assert!(sql.ends_with(",3405)"));
    }
}
