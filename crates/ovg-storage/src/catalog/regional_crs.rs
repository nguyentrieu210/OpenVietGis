//! Regional CRS evidence is derived from immutable reference fixtures and
//! independently replayed bounded reprojection chains, never caller assertions.
use super::db_error;
use super::fixtures::{materialize_fixture, regional_controls, Provenance};
use crate::editing::domain::sha256_py_json;
use ovg_core::crs::{reference_transformation, regional_datum_transformation, FixtureRequirement, TransformationProfile};
use ovg_core::AppError;
use serde_json::{json, Value};
use sqlx::{Postgres, Row, Transaction};
use uuid::Uuid;

fn outside() -> AppError {
    AppError::validation(
        "TRANSFORMATION_OUT_OF_AREA",
        "The source is outside the approved operation area or evidence.",
    )
}
fn satisfies(p: Provenance, r: FixtureRequirement) -> bool {
    match r {
        FixtureRequirement::None => true,
        FixtureRequirement::QuangNinh => {
            matches!(p, Provenance::QuangNinhCircular | Provenance::QuangNinhHvs)
        }
        FixtureRequirement::QuangNinhHvs => p == Provenance::QuangNinhHvs,
        FixtureRequirement::DaNangCircular => p == Provenance::DaNangCircular,
        FixtureRequirement::HaiPhongCircular => p == Provenance::HaiPhongCircular,
        FixtureRequirement::DongNaiCircular => p == Provenance::DongNaiCircular,
    }
}
fn trusted(p:Provenance)->bool {
    matches!(p,Provenance::QuangNinhCircular|Provenance::QuangNinhHvs|Provenance::DaNangCircular|Provenance::HaiPhongCircular|Provenance::DongNaiCircular)
}
fn effective_profile(p:Provenance,base:&'static TransformationProfile)->Result<&'static TransformationProfile,AppError>{
    if satisfies(p,base.fixture_requirement){return Ok(base)}
    let requirement=match p{Provenance::DaNangCircular=>FixtureRequirement::DaNangCircular,Provenance::HaiPhongCircular=>FixtureRequirement::HaiPhongCircular,Provenance::DongNaiCircular=>FixtureRequirement::DongNaiCircular,_=>return Err(outside())};
    regional_datum_transformation(base.source_code,base.target_code,requirement).ok_or_else(outside)
}
async fn check_independent_controls(tx:&mut Transaction<'_,Postgres>,p:Provenance,profile:&TransformationProfile)->Result<(),AppError>{
    let Some(controls)=regional_controls(p) else{return Ok(())};
    let input=if profile.source_code==4756{"ST_SetSRID(ST_MakePoint(v.x,v.y),4756)"}else{"ST_SetSRID(ST_MakePoint(v.wx,v.wy),4326)"};
    let expected=if profile.target_code==4326{"ST_MakePoint(v.wx,v.wy)"}else{"ST_MakePoint(v.x,v.y)"};
    let expression=profile_expression(profile,input);
    let pairs=json!(controls.iter().map(|(a,b)|json!({"x":a.0,"y":a.1,"wx":b.0,"wy":b.1})).collect::<Vec<_>>());
    let sql=format!("SELECT bool_and(ST_Distance(ST_SetSRID({expression},4326)::geography,ST_SetSRID({expected},4326)::geography)<=$2) FROM jsonb_to_recordset($1::jsonb) v(x double precision,y double precision,wx double precision,wy double precision)");
    let valid:Option<bool>=sqlx::query_scalar(&sql).bind(pairs).bind(profile.tolerance).fetch_one(&mut **tx).await.map_err(db_error)?;
    if valid!=Some(true){return Err(outside())}Ok(())
}
// Geometry expression is an internal SQL alias, and every operation is static.
pub(crate) fn profile_expression(profile: &TransformationProfile, geometry: &str) -> String {
    let mut expr = geometry.to_owned();
    for step in profile.steps {
        let function = if step.inverse {
            "ST_InverseTransformPipeline"
        } else {
            "ST_TransformPipeline"
        };
        expr = format!(
            "{function}({expr}, '{}', {})",
            step.pipeline, step.target_code
        );
    }
    expr
}
fn domain_expression(profile: &TransformationProfile, geometry: &str) -> String {
    let area = profile.geographic_bounds;
    let check = |expr: &str, srid: u32| {
        format!(
            "ST_CoveredBy({expr},ST_MakeEnvelope({:?},{:?},{:?},{:?},{srid}))",
            area.min_x, area.min_y, area.max_x, area.max_y
        )
    };
    let mut expr = geometry.to_owned();
    let mut conditions = Vec::new();
    if matches!(profile.source_code, 4326 | 4756) {
        conditions.push(check(&expr, profile.source_code))
    }
    for step in profile.steps {
        let function = if step.inverse {
            "ST_InverseTransformPipeline"
        } else {
            "ST_TransformPipeline"
        };
        expr = format!(
            "{function}({expr}, '{}', {})",
            step.pipeline, step.target_code
        );
        if matches!(step.target_code, 4326 | 4756) {
            conditions.push(check(&expr, step.target_code))
        }
    }
    if conditions.is_empty() {
        "false".into()
    } else {
        conditions.join(" AND ")
    }
}
async fn check_domain(
    tx: &mut Transaction<'_, Postgres>,
    org: Uuid,
    project: Uuid,
    source: Uuid,
    profile: &TransformationProfile,
) -> Result<(), AppError> {
    let predicate = domain_expression(profile, "f.geometry");
    let sql = format!(
        "SELECT NOT EXISTS(SELECT 1 FROM spatial.reference_features f
      WHERE f.organization_id=$1 AND f.project_id=$2 AND f.source_id=$3 AND
      (f.geometry_state='present' AND f.geometry IS NOT NULL AND ST_SRID(f.geometry)={}
       AND ST_IsValid(f.geometry) AND NOT ST_IsEmpty(f.geometry) AND ST_Zmflag(f.geometry)=0
       AND ({predicate})) IS NOT TRUE)",
        profile.source_code
    );
    let valid: bool = sqlx::query_scalar(&sql)
        .bind(org)
        .bind(project)
        .bind(source)
        .fetch_one(&mut **tx)
        .await
        .map_err(db_error)?;
    if !valid {
        return Err(outside());
    }
    Ok(())
}
struct Evidence {
    provenance: Provenance,
    content: String,
    schema: String,
    crs: i32,
    count: i32,
}
async fn verify_reference(
    tx: &mut Transaction<'_, Postgres>,
    org: Uuid,
    project: Uuid,
    source: Uuid,
) -> Result<Evidence, AppError> {
    let row = sqlx::query(
        "SELECT reference_key,fixture_version,manifest_digest,content_revision,schema_revision,
      source_crs,stored_crs,feature_count,geometry_family,dimensions FROM spatial.data_sources
      WHERE organization_id=$1 AND project_id=$2 AND id=$3 AND kind='reference_fixture'
      AND lifecycle='active' AND validation_state='ready' FOR SHARE",
    )
    .bind(org)
    .bind(project)
    .bind(source)
    .fetch_optional(&mut **tx)
    .await
    .map_err(db_error)?
    .ok_or_else(outside)?;
    let key: String = row.try_get("reference_key").map_err(db_error)?;
    let mat = materialize_fixture(&key, None).map_err(|_| outside())?;
    let crs: i32 = row.try_get("stored_crs").map_err(db_error)?;
    let count: i32 = row.try_get("feature_count").map_err(db_error)?;
    let content: String = row.try_get("content_revision").map_err(db_error)?;
    let schema: String = row.try_get("schema_revision").map_err(db_error)?;
    if mat.validation_state != "ready"
        || !trusted(mat.spec.provenance)
        || row
            .try_get::<String, _>("fixture_version")
            .map_err(db_error)?
            != "r003-reference-v1"
        || row
            .try_get::<String, _>("manifest_digest")
            .map_err(db_error)?
            != mat.manifest_digest
        || content != mat.content_revision
        || schema != mat.schema_revision
        || mat.spec.stored_crs != Some(crs)
        || row
            .try_get::<Option<i32>, _>("source_crs")
            .map_err(db_error)?
            != Some(crs)
        || row
            .try_get::<String, _>("geometry_family")
            .map_err(db_error)?
            != mat.spec.expected_family
        || row.try_get::<String, _>("dimensions").map_err(db_error)? != mat.spec.expected_dimensions
        || count <= 0
        || usize::try_from(count).ok() != Some(mat.records.len())
    {
        return Err(outside());
    }
    let fields=json!(mat.fields.iter().map(|f|json!({"field_id":f.field_id,"name":f.name,"ordinal":f.ordinal,"type":f.field_type,"nullable":f.nullable,"schema_revision":f.schema_revision})).collect::<Vec<_>>());
    let actual:Value=sqlx::query_scalar("SELECT COALESCE(jsonb_agg(jsonb_build_object(
      'field_id',field_id,'name',name,'ordinal',ordinal,'type',type,'nullable',nullable,'schema_revision',schema_revision)
      ORDER BY ordinal,field_id),'[]'::jsonb) FROM spatial.source_fields WHERE organization_id=$1 AND project_id=$2 AND source_id=$3")
      .bind(org).bind(project).bind(source).fetch_one(&mut **tx).await.map_err(db_error)?;
    if actual != fields {
        return Err(outside());
    }
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
    if !valid {
        return Err(outside());
    }
    Ok(Evidence {
        provenance: mat.spec.provenance,
        content,
        schema,
        crs,
        count,
    })
}
// A chain is verified from its immutable root forward. Each hop must be a
// succeeded, pinned Reproject job, with exact manifest, feature and schema
// equality. No imported/manual/edited intermediate is trusted. A hard depth
// limit and cycle rejection bound the work even for malformed database rows.
struct ReprojectHop {
    source:Uuid,
    parent:Uuid,
    crs:i32,
    content:String,
    schema:String,
    count:i32,
    refs:Value,
}
async fn check_replay(tx:&mut Transaction<'_,Postgres>,org:Uuid,project:Uuid,
    hop:&ReprojectHop,expr:&str)->Result<(),AppError>{
    let source=hop.source;let origin=hop.parent;let content=&hop.content;let schema=&hop.schema;
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
        let valid: bool = sqlx::query_scalar(&sql)
            .bind(org)
            .bind(project)
            .bind(source)
            .bind(origin)
            .bind(content)
            .bind(schema)
            .fetch_one(&mut **tx)
            .await
            .map_err(db_error)?;
        if !valid {
            return Err(outside());
        }

    Ok(())
}
async fn datum_expression_from_native(tx:&mut Transaction<'_,Postgres>,org:Uuid,project:Uuid,
    source:Uuid,native:u32,provenance:Provenance,geometry:&str)->Result<String,AppError>{
    let projection=crate::native_projection::NativeProjection::new(native,4756)?;
    projection.verify_source(tx,org,project,source).await?;
    let profile=effective_profile(provenance,reference_transformation(4756,4326).ok_or_else(outside)?)?;
    check_independent_controls(tx,provenance,profile).await?;
    let geo=projection.expression("f.geometry");
    let predicate=domain_expression(profile,&geo);
    let sql=format!("SELECT NOT EXISTS(SELECT 1 FROM spatial.reference_features f WHERE f.organization_id=$1 AND f.project_id=$2 AND f.source_id=$3 AND ({predicate}) IS NOT TRUE)");
    let valid:bool=sqlx::query_scalar(&sql).bind(org).bind(project).bind(source).fetch_one(&mut **tx).await.map_err(db_error)?;
    if !valid{return Err(outside());}
    Ok(profile_expression(profile,&projection.expression(geometry)))
}
async fn replay_expression(tx:&mut Transaction<'_,Postgres>,org:Uuid,project:Uuid,
    source:Uuid,a:u32,b:u32,provenance:Provenance)->Result<String,AppError>{
    if crate::native_projection::is_native_pair(a,b){
        let projection=crate::native_projection::NativeProjection::new(a,b)?;
        projection.verify_source(tx,org,project,source).await?;
        return Ok(projection.expression("f.geometry"));
    }
    if ovg_core::crs::is_extended_native_vn2000(a)&&b==4326 {
        return datum_expression_from_native(tx,org,project,source,a,provenance,"f.geometry").await;
    }
    let profile=effective_profile(provenance,reference_transformation(a,b).ok_or_else(outside)?)?;
    // Do not confer regional provenance on ordinary WGS84/web copies.
    if profile.fixture_requirement==FixtureRequirement::None{return Err(outside());}
    check_independent_controls(tx,provenance,profile).await?;
    check_domain(tx,org,project,source,profile).await?;
    Ok(profile_expression(profile,"f.geometry"))
}
async fn verify_chain(tx:&mut Transaction<'_,Postgres>,org:Uuid,project:Uuid,
    source:Uuid)->Result<Evidence,AppError>{
    let mut current=source;
    let mut seen=std::collections::HashSet::new();
    let mut hops=Vec::new();
    let root=loop{
        if !seen.insert(current){return Err(outside());}
        let row=sqlx::query("SELECT kind,stored_crs,content_revision,schema_revision,manifest_digest,feature_count FROM spatial.data_sources
            WHERE organization_id=$1 AND project_id=$2 AND id=$3 AND lifecycle='active' AND validation_state='ready' FOR SHARE")
            .bind(org).bind(project).bind(current).fetch_optional(&mut **tx).await.map_err(db_error)?.ok_or_else(outside)?;
        let kind:String=row.try_get("kind").map_err(db_error)?;
        if kind=="reference_fixture"{break verify_reference(tx,org,project,current).await?;}
        if kind!="derived_processing"||hops.len()>=8{return Err(outside());}
        let crs:i32=row.try_get("stored_crs").map_err(db_error)?;
        let content:String=row.try_get("content_revision").map_err(db_error)?;
        let schema:String=row.try_get("schema_revision").map_err(db_error)?;
        let manifest:String=row.try_get("manifest_digest").map_err(db_error)?;
        let count:i32=row.try_get("feature_count").map_err(db_error)?;
        if !(1..=100).contains(&count){return Err(outside());}
        let jobs=sqlx::query("SELECT id,input_refs,parameters,output_content_revision,feature_count FROM processing.jobs WHERE organization_id=$1 AND project_id=$2 AND output_source_id=$3 AND status='succeeded' AND algorithm_id='reproject' AND output_crs=$4 LIMIT 2 FOR SHARE")
            .bind(org).bind(project).bind(current).bind(crs).fetch_all(&mut **tx).await.map_err(db_error)?;
        if jobs.len()!=1{return Err(outside());}
        let job=&jobs[0];
        let id:Uuid=job.try_get("id").map_err(db_error)?;
        let refs:Value=job.try_get("input_refs").map_err(db_error)?;
        let params:Value=job.try_get("parameters").map_err(db_error)?;
        if job.try_get::<Option<String>,_>("output_content_revision").map_err(db_error)?.as_deref()!=Some(content.as_str())
          ||job.try_get::<Option<i32>,_>("feature_count").map_err(db_error)?!=Some(count)
          ||params!=json!({"target_srid":crs})
          ||manifest!=sha256_py_json(&json!({"job_id":id.to_string(),"refs":refs,"params":params})){return Err(outside());}
        let parent=refs["input_source_id"].as_str().and_then(|s|Uuid::parse_str(s).ok()).ok_or_else(outside)?;
        hops.push(ReprojectHop{source:current,parent,crs,content,schema,count,refs});
        current=parent;
    };
    let mut verified=root;
    for hop in hops.iter().rev(){
        if hop.refs["input_content_revision"].as_str()!=Some(verified.content.as_str())
            ||hop.refs["input_schema_revision"].as_str()!=Some(verified.schema.as_str())
            ||hop.refs["input_stored_crs"].as_i64()!=Some(i64::from(verified.crs))
            ||hop.count!=verified.count||hop.schema!=verified.schema{return Err(outside());}
        let expr=replay_expression(tx,org,project,hop.parent,u32::try_from(verified.crs).map_err(|_|outside())?,
            u32::try_from(hop.crs).map_err(|_|outside())?,verified.provenance).await?;
        check_replay(tx,org,project,hop,&expr).await?;
        verified=Evidence{provenance:verified.provenance,content:hop.content.clone(),schema:hop.schema.clone(),crs:hop.crs,count:hop.count};
    }
    Ok(verified)
}
pub(crate) struct VerifiedDatumRead {
    pub(crate) expression:String,
    pub(crate) wire:Value,
    pub(crate) bounds:ovg_core::crs::CoordinateBounds,
}
pub(crate) async fn verify_extended_datum_read(tx:&mut Transaction<'_,Postgres>,org:Uuid,project:Uuid,source:Uuid,native:u32,geometry:&str)->Result<VerifiedDatumRead,AppError>{
    let evidence=verify_chain(tx,org,project,source).await?;
    if !ovg_core::crs::is_extended_native_vn2000(native)||u32::try_from(evidence.crs).ok()!=Some(native){return Err(outside());}
    let profile=effective_profile(evidence.provenance,reference_transformation(4756,4326).ok_or_else(outside)?)?;
    let expression=datum_expression_from_native(tx,org,project,source,native,evidence.provenance,geometry).await?;
    let mut wire=profile.to_wire();wire["source_code"]=json!(native);wire["operation_id"]=json!("verified-native-vn2000-to-wgs84");
    wire["method"]=json!("inverse native projection then verified EPSG:6960 datum operation");
    Ok(VerifiedDatumRead{expression,wire,bounds:profile.geographic_bounds})
}
pub(crate) async fn verify_extended_datum_source(tx:&mut Transaction<'_,Postgres>,org:Uuid,project:Uuid,source:Uuid,native:u32,geometry:&str)->Result<String,AppError>{
    let evidence=verify_chain(tx,org,project,source).await?;
    if !ovg_core::crs::is_extended_native_vn2000(native)||u32::try_from(evidence.crs).ok()!=Some(native){return Err(outside());}
    datum_expression_from_native(tx,org,project,source,native,evidence.provenance,geometry).await
}
pub(crate) async fn verify_regional_source(
    tx:&mut Transaction<'_,Postgres>,org:Uuid,project:Uuid,source:Uuid,profile:&'static TransformationProfile
)->Result<&'static TransformationProfile,AppError>{
    if profile.fixture_requirement==FixtureRequirement::None{return Ok(profile);}
    let verified=verify_chain(tx,org,project,source).await?;
    if u32::try_from(verified.crs).ok()!=Some(profile.source_code){return Err(outside());}
    let profile=effective_profile(verified.provenance,profile)?;
    check_independent_controls(tx,verified.provenance,profile).await?;
    check_domain(tx,org,project,source,profile).await?;
    Ok(profile)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn verified_scope_and_composite_pipeline() {
        assert!(!satisfies(
            Provenance::Synthetic,
            FixtureRequirement::QuangNinh
        ));
        assert!(satisfies(
            Provenance::QuangNinhCircular,
            FixtureRequirement::QuangNinh
        ));
        assert!(!satisfies(
            Provenance::QuangNinhCircular,
            FixtureRequirement::QuangNinhHvs
        ));
        assert!(satisfies(
            Provenance::QuangNinhHvs,
            FixtureRequirement::QuangNinhHvs
        ));
        let p = reference_transformation(5899, 4326).unwrap();
        assert!(profile_expression(p, "geometry")
            .contains("ST_TransformPipeline(ST_InverseTransformPipeline("));
        assert!(domain_expression(p, "geometry").contains(",4756)"));
        assert!(domain_expression(p, "geometry").contains(",4326)"));
    }
}
