//! Regional CRS evidence is derived from immutable reference fixtures and
//! independently checked one-hop reprojections, never caller assertions.
use ovg_core::crs::{reference_transformation, FixtureRequirement, TransformationProfile};
use ovg_core::AppError;
use serde_json::{json, Value};
use sqlx::{Postgres, Row, Transaction};
use uuid::Uuid;
use super::db_error;
use super::fixtures::{materialize_fixture, Provenance};
use crate::editing::domain::sha256_py_json;

fn outside() -> AppError {
    AppError::validation("TRANSFORMATION_OUT_OF_AREA", "Regional CRS conversion requires verified reference evidence within the approved area.")
}
fn satisfies(p:Provenance,r:FixtureRequirement)->bool {
    match r {
        FixtureRequirement::None=>true,
        FixtureRequirement::QuangNinh=>matches!(p,Provenance::QuangNinhCircular|Provenance::QuangNinhHvs),
        FixtureRequirement::QuangNinhHvs=>p==Provenance::QuangNinhHvs,
    }
}
// Geometry expression is an internal SQL alias, and every operation is static.
pub(crate) fn profile_expression(profile:&TransformationProfile,geometry:&str)->String {
    let mut expr=geometry.to_owned();
    for step in profile.steps {
        let function=if step.inverse {"ST_InverseTransformPipeline"}else{"ST_TransformPipeline"};
        expr=format!("{function}({expr}, '{}', {})",step.pipeline,step.target_code);
    }
    expr
}
fn domain_expression(profile:&TransformationProfile,geometry:&str)->String {
    let area=profile.geographic_bounds;
    let check=|expr:&str,srid:u32|format!("ST_CoveredBy({expr},ST_MakeEnvelope({:?},{:?},{:?},{:?},{srid}))",area.min_x,area.min_y,area.max_x,area.max_y);
    let mut expr=geometry.to_owned();let mut conditions=Vec::new();
    if matches!(profile.source_code,4326|4756){conditions.push(check(&expr,profile.source_code))}
    for step in profile.steps {
        let function=if step.inverse {"ST_InverseTransformPipeline"}else{"ST_TransformPipeline"};
        expr=format!("{function}({expr}, '{}', {})",step.pipeline,step.target_code);
        if matches!(step.target_code,4326|4756){conditions.push(check(&expr,step.target_code))}
    }
    if conditions.is_empty(){"false".into()}else{conditions.join(" AND ")}
}
async fn check_domain(tx:&mut Transaction<'_,Postgres>,org:Uuid,project:Uuid,source:Uuid,profile:&TransformationProfile)->Result<(),AppError>{
    let predicate=domain_expression(profile,"f.geometry");
    let sql=format!("SELECT NOT EXISTS(SELECT 1 FROM spatial.reference_features f
      WHERE f.organization_id=$1 AND f.project_id=$2 AND f.source_id=$3 AND
      (f.geometry_state='present' AND f.geometry IS NOT NULL AND ST_SRID(f.geometry)={}
       AND ST_IsValid(f.geometry) AND NOT ST_IsEmpty(f.geometry) AND ST_Zmflag(f.geometry)=0
       AND ({predicate})) IS NOT TRUE)",profile.source_code);
    let valid:bool=sqlx::query_scalar(&sql).bind(org).bind(project).bind(source).fetch_one(&mut **tx).await.map_err(db_error)?;
    if !valid{return Err(outside())} Ok(())
}
struct Evidence {provenance:Provenance,content:String,schema:String,crs:i32,count:i32}
async fn verify_reference(tx:&mut Transaction<'_,Postgres>,org:Uuid,project:Uuid,source:Uuid)->Result<Evidence,AppError>{
    let row=sqlx::query("SELECT reference_key,fixture_version,manifest_digest,content_revision,schema_revision,
      source_crs,stored_crs,feature_count,geometry_family,dimensions FROM spatial.data_sources
      WHERE organization_id=$1 AND project_id=$2 AND id=$3 AND kind='reference_fixture'
      AND lifecycle='active' AND validation_state='ready' FOR SHARE")
      .bind(org).bind(project).bind(source).fetch_optional(&mut **tx).await.map_err(db_error)?.ok_or_else(outside)?;
    let key:String=row.try_get("reference_key").map_err(db_error)?;
    let mat=materialize_fixture(&key,None).map_err(|_|outside())?;
    let crs:i32=row.try_get("stored_crs").map_err(db_error)?;
    let count:i32=row.try_get("feature_count").map_err(db_error)?;
    let content:String=row.try_get("content_revision").map_err(db_error)?;
    let schema:String=row.try_get("schema_revision").map_err(db_error)?;
    if mat.validation_state!="ready"||!satisfies(mat.spec.provenance,FixtureRequirement::QuangNinh)
      ||row.try_get::<String,_>("fixture_version").map_err(db_error)?!="r003-reference-v1"
      ||row.try_get::<String,_>("manifest_digest").map_err(db_error)?!=mat.manifest_digest
      ||content!=mat.content_revision||schema!=mat.schema_revision||mat.spec.stored_crs!=Some(crs)
      ||row.try_get::<Option<i32>,_>("source_crs").map_err(db_error)?!=Some(crs)
      ||row.try_get::<String,_>("geometry_family").map_err(db_error)?!=mat.spec.expected_family
      ||row.try_get::<String,_>("dimensions").map_err(db_error)?!=mat.spec.expected_dimensions
      ||count<=0||usize::try_from(count).ok()!=Some(mat.records.len()){return Err(outside())}
    let fields=json!(mat.fields.iter().map(|f|json!({"field_id":f.field_id,"name":f.name,"ordinal":f.ordinal,"type":f.field_type,"nullable":f.nullable,"schema_revision":f.schema_revision})).collect::<Vec<_>>());
    let actual:Value=sqlx::query_scalar("SELECT COALESCE(jsonb_agg(jsonb_build_object(
      'field_id',field_id,'name',name,'ordinal',ordinal,'type',type,'nullable',nullable,'schema_revision',schema_revision)
      ORDER BY ordinal,field_id),'[]'::jsonb) FROM spatial.source_fields WHERE organization_id=$1 AND project_id=$2 AND source_id=$3")
      .bind(org).bind(project).bind(source).fetch_one(&mut **tx).await.map_err(db_error)?;
    if actual!=fields{return Err(outside())}
    let records=json!(mat.records.iter().map(|r|json!({"feature_id":r.feature_id,"attributes":r.attributes,"geometry_state":r.geometry_state,"wkt":r.geometry.as_ref().map(|g|g.wkt.as_str()),"srid":r.geometry.as_ref().and_then(|g|g.srid)})).collect::<Vec<_>>());
    let valid:bool=sqlx::query_scalar("WITH expected AS (
      SELECT e.feature_id,e.attributes,e.geometry_state,ST_GeomFromText(e.wkt,e.srid) AS geometry
      FROM jsonb_to_recordset($4::jsonb) e(feature_id text,attributes jsonb,geometry_state text,wkt text,srid integer)
      ),actual AS(SELECT feature_id,attributes,geometry_state,geometry,content_revision,schema_revision
        FROM spatial.reference_features WHERE organization_id=$1 AND project_id=$2 AND source_id=$3)
      SELECT NOT EXISTS(SELECT 1 FROM expected e FULL OUTER JOIN actual a ON e.feature_id=a.feature_id
      WHERE e.feature_id IS NULL OR a.feature_id IS NULL OR e.attributes IS DISTINCT FROM a.attributes
      OR e.geometry_state IS DISTINCT FROM a.geometry_state OR ST_AsEWKB(e.geometry,'NDR') IS DISTINCT FROM ST_AsEWKB(a.geometry,'NDR')
      OR a.content_revision IS DISTINCT FROM $5 OR a.schema_revision IS DISTINCT FROM $6)")
      .bind(org).bind(project).bind(source).bind(records).bind(&content).bind(&schema)
      .fetch_one(&mut **tx).await.map_err(db_error)?;
    if !valid{return Err(outside())}
    Ok(Evidence{provenance:mat.spec.provenance,content,schema,crs,count})
}
pub(crate) async fn verify_regional_source(tx:&mut Transaction<'_,Postgres>,org:Uuid,project:Uuid,source:Uuid,profile:&TransformationProfile)->Result<(),AppError>{
    if profile.fixture_requirement==FixtureRequirement::None{return Ok(())}
    let row=sqlx::query("SELECT kind,stored_crs,content_revision,schema_revision,manifest_digest,feature_count
      FROM spatial.data_sources WHERE organization_id=$1 AND project_id=$2 AND id=$3
      AND lifecycle='active' AND validation_state='ready' FOR SHARE")
      .bind(org).bind(project).bind(source).fetch_optional(&mut **tx).await.map_err(db_error)?.ok_or_else(outside)?;
    let crs:i32=row.try_get("stored_crs").map_err(db_error)?;
    if u32::try_from(crs).ok()!=Some(profile.source_code){return Err(outside())}
    let kind:String=row.try_get("kind").map_err(db_error)?;
    let provenance=if kind=="reference_fixture"{verify_reference(tx,org,project,source).await?.provenance}
    else if kind=="derived_processing"{
      let content:String=row.try_get("content_revision").map_err(db_error)?;
      let schema:String=row.try_get("schema_revision").map_err(db_error)?;
      let manifest:String=row.try_get("manifest_digest").map_err(db_error)?;
      let count:i32=row.try_get("feature_count").map_err(db_error)?;
      let jobs=sqlx::query("SELECT id,input_refs,parameters,output_content_revision,feature_count
        FROM processing.jobs WHERE organization_id=$1 AND project_id=$2 AND output_source_id=$3
        AND status='succeeded' AND algorithm_id='reproject' AND output_crs=$4 LIMIT 2")
        .bind(org).bind(project).bind(source).bind(crs).fetch_all(&mut **tx).await.map_err(db_error)?;
      if jobs.len()!=1{return Err(outside())}
      let job=&jobs[0];let id:Uuid=job.try_get("id").map_err(db_error)?;
      let refs:Value=job.try_get("input_refs").map_err(db_error)?;let params:Value=job.try_get("parameters").map_err(db_error)?;
      if job.try_get::<Option<String>,_>("output_content_revision").map_err(db_error)?.as_deref()!=Some(content.as_str())
        ||job.try_get::<Option<i32>,_>("feature_count").map_err(db_error)?!=Some(count)
        ||params!=json!({"target_srid":crs})||manifest!=sha256_py_json(&json!({"job_id":id.to_string(),"refs":refs,"params":params})){return Err(outside())}
      let origin=refs["input_source_id"].as_str().and_then(|v|Uuid::parse_str(v).ok()).ok_or_else(outside)?;
      if origin==source{return Err(outside())}
      let verified=verify_reference(tx,org,project,origin).await?;
      if refs["input_content_revision"].as_str()!=Some(verified.content.as_str())||refs["input_schema_revision"].as_str()!=Some(verified.schema.as_str())
        ||refs["input_stored_crs"].as_i64()!=Some(i64::from(verified.crs))||count!=verified.count||schema!=verified.schema{return Err(outside())}
      let prior=reference_transformation(u32::try_from(verified.crs).map_err(|_|outside())?,profile.source_code).ok_or_else(outside)?;
      if !satisfies(verified.provenance,prior.fixture_requirement){return Err(outside())}
      check_domain(tx,org,project,origin,prior).await?;
      let expr=profile_expression(prior,"f.geometry");
      let sql=format!("WITH expected AS(SELECT f.feature_id,f.attributes,{expr} AS geometry FROM spatial.reference_features f
        WHERE f.organization_id=$1 AND f.project_id=$2 AND f.source_id=$4),
        actual AS(SELECT feature_id,attributes,geometry,geometry_state,content_revision,schema_revision FROM spatial.reference_features
        WHERE organization_id=$1 AND project_id=$2 AND source_id=$3)
        SELECT NOT EXISTS(SELECT 1 FROM expected e FULL OUTER JOIN actual a ON e.feature_id=a.feature_id
          WHERE e.feature_id IS NULL OR a.feature_id IS NULL OR a.attributes IS DISTINCT FROM e.attributes
          OR a.geometry_state IS DISTINCT FROM 'present' OR ST_AsEWKB(a.geometry,'NDR') IS DISTINCT FROM ST_AsEWKB(e.geometry,'NDR')
          OR a.content_revision IS DISTINCT FROM $5 OR a.schema_revision IS DISTINCT FROM $6)
        AND NOT EXISTS((SELECT field_id,name,ordinal,type,nullable,schema_revision FROM spatial.source_fields WHERE organization_id=$1 AND project_id=$2 AND source_id=$3
          EXCEPT SELECT field_id,name,ordinal,type,nullable,schema_revision FROM spatial.source_fields WHERE organization_id=$1 AND project_id=$2 AND source_id=$4)
          UNION ALL (SELECT field_id,name,ordinal,type,nullable,schema_revision FROM spatial.source_fields WHERE organization_id=$1 AND project_id=$2 AND source_id=$4
          EXCEPT SELECT field_id,name,ordinal,type,nullable,schema_revision FROM spatial.source_fields WHERE organization_id=$1 AND project_id=$2 AND source_id=$3))");
      let valid:bool=sqlx::query_scalar(&sql).bind(org).bind(project).bind(source).bind(origin).bind(content).bind(schema).fetch_one(&mut **tx).await.map_err(db_error)?;
      if !valid{return Err(outside())} verified.provenance
    }else{return Err(outside())};
    if !satisfies(provenance,profile.fixture_requirement){return Err(outside())}
    check_domain(tx,org,project,source,profile).await
}
#[cfg(test)] mod tests{
 use super::*;
 #[test] fn verified_scope_and_composite_pipeline(){
  assert!(!satisfies(Provenance::Synthetic,FixtureRequirement::QuangNinh));
  assert!(satisfies(Provenance::QuangNinhCircular,FixtureRequirement::QuangNinh));
  assert!(!satisfies(Provenance::QuangNinhCircular,FixtureRequirement::QuangNinhHvs));
  assert!(satisfies(Provenance::QuangNinhHvs,FixtureRequirement::QuangNinhHvs));
  let p=reference_transformation(5899,4326).unwrap();
  assert!(profile_expression(p,"geometry").contains("ST_TransformPipeline(ST_InverseTransformPipeline("));
  assert!(domain_expression(p,"geometry").contains(",4756)"));
  assert!(domain_expression(p,"geometry").contains(",4326)"));
 }
}
