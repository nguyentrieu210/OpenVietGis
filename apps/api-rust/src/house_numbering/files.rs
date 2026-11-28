//! Native E15 file-operation HTTP boundary. Only codec/render work is delegated.
use std::collections::{HashMap, HashSet};
use std::path::{Path as FsPath, PathBuf};
use axum::body::{Body, Bytes};
use axum::extract::{Path, Query, State};
use axum::http::{header, HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use ovg_core::AppError;
use ovg_storage::catalog::ProjectRead;
use ovg_storage::house_numbering as store;
use serde::Deserialize;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use uuid::Uuid;
use super::{audit_actor, csrf, decode, module_edit, module_manage, module_read, path_id, scope_ids, HnResult, HouseNumberingError, BASE};
use crate::identity::HumanSession;
use crate::AppState;

const MAX_IMPORT: usize = 25 * 1024 * 1024;

pub(super) fn router() -> Router<AppState> {
    Router::new()
        .route(&format!("{BASE}/imports"), post(import_evidence))
        .route(&format!("{BASE}/documents"), post(issue_document))
        .route(&format!("{BASE}/documents/batch"), post(issue_batch))
        .route(&format!("{BASE}/exports"), post(create_export))
        .route(&format!("{BASE}/proposals/{{proposal_id}}/packets/{{kind}}"), get(packet))
}

fn invalid(code: &'static str, message: impl Into<String>) -> HouseNumberingError {
    AppError::validation(code, message.into()).into()
}
fn io_error() -> AppError {
    AppError::internal("house-numbering artifact storage failed")
}

/// Store in the canonical module namespace using an immutable UUID directory.
/// Remove staged files on failure, retain only once the database commits.
struct PendingArtifact {
    dir: PathBuf,
    id: Uuid,
    meta: Value,
    sha: String,
    committed: bool,
}
impl Drop for PendingArtifact {
    fn drop(&mut self) {
        if !self.committed {
            let _ = std::fs::remove_dir_all(&self.dir);
        }
    }
}
impl PendingArtifact {
    async fn create(root: &FsPath, scope: &ProjectRead<'_>, namespace: &str, filename: &str, media: &str, bytes: &[u8]) -> Result<Self, AppError> {
        let filename: String = FsPath::new(filename).file_name().and_then(|p| p.to_str()).unwrap_or("artifact.bin")
            .chars().take(180).map(|c| if c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-') {c} else {'-'}).collect();
        let name=filename.trim_matches(&['.', '-', '_'][..]);
        let name=if name.is_empty() {"artifact.bin"} else {name};
        let id=Uuid::new_v4();
        let key=format!("extensions/{}/{}/{namespace}/{id}/{name}",scope.organization_id(),scope.project_id());
        let path=root.join(&key);
        let dir=path.parent().ok_or_else(io_error)?.to_path_buf();
        let parent=dir.parent().ok_or_else(io_error)?;
        tokio::fs::create_dir_all(parent).await.map_err(|_|io_error())?;
        let mut builder=std::fs::DirBuilder::new();
        #[cfg(unix)]
        {
            use std::os::unix::fs::DirBuilderExt;
            builder.mode(0o700);
        }
        builder.create(&dir).map_err(|_|io_error())?;
        let sha=format!("{:x}",Sha256::digest(bytes));
        let meta=json!({"id":id,"filename":name,"content_type":media,"byte_size":bytes.len(),"sha256":sha});
        let pending=Self{dir,id,meta,sha,committed:false};
        let mut disk_meta=pending.meta.clone();
        disk_meta["storage_key"]=json!(key);
        tokio::fs::write(&path,bytes).await.map_err(|_|io_error())?;
        let metadata=pending.dir.join(format!("{name}.json"));
        tokio::fs::write(metadata,serde_json::to_vec(&disk_meta).map_err(|_|io_error())?)
            .await.map_err(|_|io_error())?;
        Ok(pending)
    }
    fn finish(mut self) -> Value {
        self.committed=true;
        self.meta.clone()
    }
}

async fn import_evidence(
    State(state): State<AppState>, auth: HumanSession, headers: HeaderMap,
    Path(params): Path<HashMap<String,String>>, body: Body,
) -> HnResult<(StatusCode,Json<Value>)> {
    csrf(&state,&auth,&headers)?;
    let (org,project)=scope_ids(&params)?;
    let mut edit=module_edit(&state,&auth,org,project).await?;
    let mime=headers.get(header::CONTENT_TYPE).and_then(|v|v.to_str().ok()).ok_or(HouseNumberingError::InvalidRequest)?;
    let raw=axum::body::to_bytes(body,MAX_IMPORT+65536).await
        .map_err(|_|HouseNumberingError::Detail(StatusCode::PAYLOAD_TOO_LARGE,"Import exceeds 25 MiB".to_owned()))?;
    let parts=crate::transfers::store::multipart_parts(mime,&raw).ok_or(HouseNumberingError::InvalidRequest)?;
    let mut text=HashMap::new();
    let mut upload=None;
    for (key,filename,data) in parts {
        if key=="file" {
            if upload.replace((filename.unwrap_or("import.bin"),data)).is_some(){return Err(HouseNumberingError::InvalidRequest);}
        } else {
            let v=std::str::from_utf8(data).map_err(|_|HouseNumberingError::InvalidRequest)?;
            if text.insert(key,v).is_some(){return Err(HouseNumberingError::InvalidRequest);}
        }
    }
    let (filename,data)=upload.ok_or(HouseNumberingError::InvalidRequest)?;
    if data.len()>MAX_IMPORT{return Err(HouseNumberingError::Detail(StatusCode::PAYLOAD_TOO_LARGE,"Import exceeds 25 MiB".to_owned()));}
    let get=|key:&str|text.get(key).copied().ok_or(HouseNumberingError::InvalidRequest);
    let format=get("source_format")?;
    let origin=get("origin")?;
    let default_reference=get("default_source_reference")?;
    let default_confidence=text.get("default_confidence").copied().unwrap_or("observed");
    let mapping:HashMap<String,String>=serde_json::from_str(get("mapping_json")?)
        .map_err(|_|invalid("VALIDATION_ERROR","mapping_json must be a JSON object of strings"))?;
    let source_crs=text.get("source_crs").filter(|s|!s.is_empty()).map(|s|s.parse::<i32>())
        .transpose().map_err(|_|HouseNumberingError::InvalidRequest)?;
    if !matches!(format,"csv"|"xlsx"|"geojson"|"dxf"|"shapefile_zip"){return Err(invalid("IMPORT_FORMAT","unsupported import format"));}
    if !matches!(origin,"certificate"|"cad"|"survey"|"legacy"|"manual"){return Err(invalid("IMPORT_ORIGIN","invalid evidence origin"));}
    if !matches!(default_confidence,"authoritative"|"observed"|"estimated"){return Err(invalid("IMPORT_CONFIDENCE","invalid evidence confidence"));}
    if matches!(format,"geojson"|"dxf"|"shapefile_zip"){
        let srid=source_crs.ok_or_else(||invalid("IMPORT_CRS_REQUIRED","spatial import requires an explicit source CRS"))?;
        if !store::import_crs_exists(&mut edit,srid).await? {return Err(invalid("IMPORT_CRS_UNKNOWN","spatial import CRS is unknown"));}
    }
    let result=super::file_codec::run(&json!({"action":"parse","format":format,"fingerprint_options":
        {"source_format":format,"origin":origin,"mapping":mapping,"source_crs":source_crs}}),Some(data),true).await?;
    let parsed:Value=serde_json::from_slice(&result).map_err(|_|AppError::internal("invalid import codec output"))?;
    let fp=parsed["fingerprint"].as_str().filter(|s|s.len()==64 && s.bytes().all(|b|b.is_ascii_hexdigit())).ok_or_else(||AppError::internal("invalid import fingerprint"))?;
    let rows=parsed["rows"].as_array().filter(|r|r.len()<=50000).ok_or_else(||AppError::internal("invalid import rows"))?;
    if let Some(batch)=store::import_lock(&mut edit,fp).await?{
        edit.commit().await?;
        return Ok((StatusCode::CREATED,Json(json!({"batch":batch,"replayed":true,"rejections":[]}))));
    }
    let artifact=PendingArtifact::create(&state.config.artifact_store_dir,&edit,"house-numbering-imports",filename,"application/octet-stream",data).await?;
    let response=store::stage_evidence(&mut edit,audit_actor(&auth),store::ImportEvidence{
        filename,fingerprint:fp,format,origin,source_crs,mapping:&mapping,default_reference,default_confidence,
        encoding:&parsed["encoding"],rows,artifact_id:artifact.id,
    }).await?;
    edit.commit().await?;
    artifact.finish();
    Ok((StatusCode::CREATED,Json(response)))
}

fn pdf_default()->String{"pdf".to_owned()}
#[derive(Debug,Deserialize)]
#[serde(deny_unknown_fields)]
struct DocumentRequest{
    building_id:Uuid, doc_type:String,
    #[serde(default="pdf_default")] output_format:String,
    template_revision:String,
    #[serde(default)] template_verified:bool,
    #[serde(default)] replaces_document_id:Option<Uuid>,
}
#[derive(Debug,Deserialize)]
#[serde(deny_unknown_fields)]
struct BatchRequest{
    building_ids:Vec<Uuid>,doc_type:String,
    #[serde(default="pdf_default")] output_format:String,
    template_revision:String,
    #[serde(default)] template_verified:bool,
}
fn validate_document(body:&DocumentRequest)->HnResult<()>{
    if !matches!(body.doc_type.as_str(),"inspection"|"certificate"|"house_form"|"plate")
        || !matches!(body.output_format.as_str(),"pdf"|"docx")
        || !(1..=100).contains(&body.template_revision.chars().count()){
        return Err(HouseNumberingError::InvalidRequest);
    }
    Ok(())
}
async fn issue_one(state:&AppState,auth:&HumanSession,org:Uuid,project:Uuid,body:&DocumentRequest)->HnResult<Value>{
    validate_document(body)?;
    let mut edit=module_manage(state,auth,org,project).await?;
    let snapshot=store::document_snapshot(&mut edit,body.building_id).await?;
    let number=snapshot["number_text"].as_str().unwrap_or("");
    let approved=snapshot["number_kind"]=="approved";
    let has_rule=snapshot["rule_id"].as_str().is_some();
    if body.doc_type!="inspection" && (!approved || number.is_empty() || !has_rule){
        return Err(AppError::conflict("APPROVED_NUMBER_REQUIRED","official house-number output requires approved number").into());
    }
    let official=body.template_verified && has_rule && approved
        && snapshot["authority_ref"].as_str().is_some_and(|s|!s.trim().is_empty());
    let status=if official{"official"}else{"draft"};
    let title=match body.doc_type.as_str(){
        "inspection"=>"PHIẾU KIỂM TRA THÔNG TIN NHÀ",
        "certificate"=>"HỒ SƠ XÁC NHẬN SỐ NHÀ",
        "house_form"=>"PHIẾU THÔNG TIN SỐ NHÀ",
        _=>"BIỂN SỐ NHÀ",
    };
    let show=|key:&str|snapshot[key].as_str().filter(|s|!s.is_empty()).unwrap_or("—");
    let lines=vec![format!("Mã công trình: {}",body.building_id),format!("Tuyến: {}",show("route_name")),
        format!("Số nhà: {}",if number.is_empty(){"Chưa có"}else{number}),format!("Thửa/parcel: {}",show("parcel_ref")),
        format!("Quy tắc: {}",show("rule_version")),format!("Căn cứ: {}",show("authority_ref")),
        format!("Template revision: {}",body.template_revision)];
    let format=if body.doc_type=="plate"{"pdf"}else{body.output_format.as_str()};
    let media=if format=="docx"{"application/vnd.openxmlformats-officedocument.wordprocessingml.document"}else{"application/pdf"};
    let bytes=super::file_codec::run(&json!({"action":"document","doc_type":body.doc_type,"format":format,
        "title":title,"lines":lines,"snapshot":snapshot}),None,false).await?;
    let filename=format!("{}-{}.{format}",body.doc_type,body.building_id);
    let artifact=PendingArtifact::create(&state.config.artifact_store_dir,&edit,"house-numbering-documents",&filename,media,&bytes).await?;
    let mut result=store::publish_document(&mut edit,audit_actor(auth),store::IssueDocument{
        building:body.building_id,number:snapshot["number_id"].as_str().and_then(|s|Uuid::parse_str(s).ok()),
        doc_type:&body.doc_type,template:&body.template_revision,status,replaces:body.replaces_document_id,
        artifact_id:artifact.id,artifact_sha:&artifact.sha,
    }).await?;
    edit.commit().await?;
    result["artifact"]=artifact.finish();
    Ok(result)
}
async fn issue_document(State(state):State<AppState>,auth:HumanSession,headers:HeaderMap,
    Path(params):Path<HashMap<String,String>>,body:Bytes)->HnResult<(StatusCode,Json<Value>)>{
    csrf(&state,&auth,&headers)?;
    let (org,project)=scope_ids(&params)?;
    let body:DocumentRequest=decode(&body)?;
    Ok((StatusCode::CREATED,Json(issue_one(&state,&auth,org,project,&body).await?)))
}
async fn issue_batch(State(state):State<AppState>,auth:HumanSession,headers:HeaderMap,
    Path(params):Path<HashMap<String,String>>,body:Bytes)->HnResult<(StatusCode,Json<Value>)>{
    csrf(&state,&auth,&headers)?;
    let (org,project)=scope_ids(&params)?;
    let body:BatchRequest=decode(&body)?;
    if !(1..=100).contains(&body.building_ids.len()) || body.building_ids.iter().collect::<HashSet<_>>().len()!=body.building_ids.len(){
        return Err(invalid("VALIDATION_ERROR","document batch must contain 1..100 unique buildings"));
    }
    let mut items=Vec::new();
    // Preserve legacy per-document commits and authorization checks.
    for building_id in body.building_ids {
        let request=DocumentRequest{building_id,doc_type:body.doc_type.clone(),output_format:body.output_format.clone(),
            template_revision:body.template_revision.clone(),template_verified:body.template_verified,replaces_document_id:None};
        items.push(issue_one(&state,&auth,org,project,&request).await?);
    }
    Ok((StatusCode::CREATED,Json(json!({"items":items}))))
}

#[derive(Debug,Deserialize)]
#[serde(deny_unknown_fields)]
struct ExportRequest{
    fields:Vec<String>,output_format:String,
    #[serde(default)] include_private:bool,
    #[serde(default)] purpose:Option<String>,
}
async fn create_export(State(state):State<AppState>,auth:HumanSession,headers:HeaderMap,
    Path(params):Path<HashMap<String,String>>,body:Bytes)->HnResult<(StatusCode,Json<Value>)>{
    csrf(&state,&auth,&headers)?;
    let (org,project)=scope_ids(&params)?;
    let request:ExportRequest=decode(&body)?;
    if !(1..=100).contains(&request.fields.len()) || request.purpose.as_ref().is_some_and(|v|v.chars().count()>500){
        return Err(HouseNumberingError::InvalidRequest);
    }
    if request.fields.iter().collect::<HashSet<_>>().len()!=request.fields.len(){return Err(invalid("VALIDATION_ERROR","export requires unique explicit fields"));}
    let (ext,media)=match request.output_format.as_str(){
        "csv"=>("csv","text/csv; charset=utf-8"),"geojson"=>("geojson","application/geo+json"),
        "dxf"=>("dxf","application/dxf"),"shapefile_zip"=>("zip","application/zip"),
        _=>return Err(HouseNumberingError::InvalidRequest),
    };
    let mut read=if request.include_private{module_manage(&state,&auth,org,project).await?}else{module_read(&state,&auth,org,project).await?};
    let (rows,prj)=store::export_rows(&mut read,audit_actor(&auth),&request.fields,request.include_private,request.purpose.as_deref().unwrap_or("")).await?;
    let count=rows.len();
    let bytes=super::file_codec::run(&json!({"action":"export","rows":rows,"fields":request.fields,"format":request.output_format,"prj":prj}),None,false).await?;
    let namespace=if request.include_private{"house-numbering-private-exports"}else{"house-numbering-exports"};
    let filename=format!("house-numbering-{}.{ext}",chrono::Utc::now().format("%Y%m%dT%H%M%SZ"));
    let artifact=PendingArtifact::create(&state.config.artifact_store_dir,&read,namespace,&filename,media,&bytes).await?;
    store::audit_export(&mut read,audit_actor(&auth),artifact.id,json!({"format":request.output_format,"fields":request.fields,"include_private":request.include_private,"record_count":count,"sha256":artifact.sha})).await?;
    read.commit().await?;
    let meta=artifact.finish();
    Ok((StatusCode::CREATED,Json(json!({"artifact_id":meta["id"],"filename":meta["filename"],"content_type":meta["content_type"],
        "byte_size":meta["byte_size"],"sha256":meta["sha256"],"record_count":count,"include_private":request.include_private}))))
}

async fn packet(State(state):State<AppState>,auth:HumanSession,
    Path(params):Path<HashMap<String,String>>,Query(query):Query<HashMap<String,String>>)->HnResult<Response>{
    let (org,project)=scope_ids(&params)?;
    let id=path_id(&params,"proposal_id")?;
    let kind=params.get("kind").map(String::as_str).ok_or(HouseNumberingError::InvalidRequest)?;
    let (filename,media)=match kind{
        "decision"=>("decision-draft.pdf","application/pdf"),
        "appendix"=>("numbering-appendix.pdf","application/pdf"),
        "register"=>("numbering-register.xlsx","application/vnd.openxmlformats-officedocument.spreadsheetml.sheet"),
        "plates"=>("qr-plates.pdf","application/pdf"),
        _=>return Err(HouseNumberingError::InvalidRequest),
    };
    let offset=super::hierarchy::query_int(&query,"offset",0,0,i64::MAX)?;
    let limit=super::hierarchy::query_int(&query,"limit",24,1,100)?;
    let mut read=module_read(&state,&auth,org,project).await?;
    let (proposal,mut rows)=store::packet_data(&mut read,id).await?;
    if rows.is_empty(){return Err(invalid("EMPTY_PROPOSAL","Proposal has no items"));}
    if kind=="plates" && (proposal["status"]!="approved" || proposal["rule_status"]!="authoritative" || proposal["authority_ref"].as_str().is_none_or(str::is_empty)){
        return Err(AppError::conflict("APPROVAL_REQUIRED","Approved proposal and authoritative rule are required").into());
    }
    if matches!(kind,"appendix"|"plates"){
        let start=usize::try_from(offset).unwrap_or(usize::MAX).min(rows.len());
        let end=start.saturating_add(usize::try_from(limit).unwrap_or(100)).min(rows.len());
        rows=rows[start..end].to_vec();
        if rows.is_empty(){return Err(if kind=="plates"{invalid("EMPTY_PLATE_BATCH","No houses in this page")}else{invalid("EMPTY_APPENDIX_BATCH","No houses on this appendix page")});}
    }
    if kind=="plates" && rows.iter().any(|r|r["number_kind"]!="approved" || !r["retired_at"].is_null() || r["building_state"]!="active" || r["number_text"]!=r["proposed_number"]){
        return Err(AppError::conflict("STALE_APPROVAL","An approved number was changed or retired").into());
    }
    let bytes=super::file_codec::run(&json!({"action":"packet","proposal":proposal,"rows":rows,"kind":kind,"org":org,"project":project}),None,false).await?;
    read.commit().await?;
    let mut response=(StatusCode::OK,bytes).into_response();
    response.headers_mut().insert(header::CONTENT_TYPE,axum::http::HeaderValue::from_static(media));
    response.headers_mut().insert(header::CACHE_CONTROL,axum::http::HeaderValue::from_static("private, no-store"));
    response.headers_mut().insert(header::CONTENT_DISPOSITION,axum::http::HeaderValue::from_str(&format!("attachment; filename=\"{filename}\"")).map_err(|_|AppError::internal("invalid packet filename"))?);
    Ok(response)
}
