//! HTTP host contract for trusted, compiled-in vertical modules.
//! The host owns authentication, CSRF and project error representation.
use axum::{extract::FromRequestParts,http::{request::Parts,HeaderMap},response::Response};
use ovg_core::AppError;
use ovg_module_sdk::{ModuleManifest,ModuleRegistry};
use ovg_storage::{catalog::{ProjectRead,ProjectReadPolicy},extensions,DbPool};
use std::{future::Future,path::PathBuf,pin::Pin,sync::Arc};
use uuid::Uuid;

pub trait SessionProof: Send + Sync {
    fn actor_id(&self)->Uuid;
    fn session_id(&self)->Uuid;
    fn display_name(&self)->&str;
    fn require_csrf(&self,headers:&HeaderMap)->Result<(),Response>;
}

pub type AuthFuture<'a> = Pin<Box<dyn Future<Output=Result<ModuleSession,Response>>+Send+'a>>;
pub trait HttpHost: Send + Sync {
    fn authenticate<'a>(&'a self,headers:&'a HeaderMap)->AuthFuture<'a>;
    fn project_error(&self,error:AppError)->Response;
}

pub struct ModuleSession(Box<dyn SessionProof>);
impl ModuleSession {
    pub fn from_verified(proof:impl SessionProof+'static)->Self{Self(Box::new(proof))}
    pub fn actor_id(&self)->Uuid{self.0.actor_id()}
    pub fn session_id(&self)->Uuid{self.0.session_id()}
    pub fn display_name(&self)->&str{self.0.display_name()}
    pub fn require_csrf(&self,headers:&HeaderMap)->Result<(),Response>{self.0.require_csrf(headers)}
}

#[derive(Clone)]
pub struct ModuleHttpState {
    pub db_pool:Option<DbPool>,
    pub modules:ModuleRegistry,
    pub artifact_store_dir:PathBuf,
    pub host:Arc<dyn HttpHost>,
}
impl FromRequestParts<ModuleHttpState> for ModuleSession {
    type Rejection=Response;
    async fn from_request_parts(parts:&mut Parts,state:&ModuleHttpState)->Result<Self,Self::Rejection>{
        state.host.authenticate(&parts.headers).await
    }
}
pub async fn project_read<'a>(state:&'a ModuleHttpState,auth:&ModuleSession,org:Uuid,project:Uuid,policy:ProjectReadPolicy)->Result<ProjectRead<'a>,Response>{
    let pool=state.db_pool.as_ref().ok_or_else(||state.host.project_error(AppError::service_unavailable("DATABASE_UNAVAILABLE","database unavailable")))?;
    ProjectRead::begin(pool,org,project,auth.actor_id(),auth.session_id(),policy).await.map_err(|e|state.host.project_error(e))
}
pub async fn verified_manifest(registry:&ModuleRegistry,read:&mut ProjectRead<'_>,key:&str)->Result<ModuleManifest,AppError>{
    let shipped=registry.manifest(key).ok_or_else(||AppError::not_found("official module runtime not registered"))?;
    let stored=extensions::active_manifest(read,key).await?;
    let contract=serde_json::from_value::<ModuleManifest>(stored.contract()).map_err(|_|AppError::not_found("official module activation does not match shipped runtime"))?;
    if !shipped.runtime_contract_matches(&contract,stored.enabled){return Err(AppError::not_found("official module activation does not match shipped runtime"));}
    Ok(contract)
}
