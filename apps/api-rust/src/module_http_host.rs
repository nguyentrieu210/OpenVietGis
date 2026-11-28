//! Composition root for compiled module HTTP routers.
use axum::Router;
use crate::AppState;

#[cfg(feature="house-numbering")]
mod enabled {
    use super::*;
    use axum::{http::HeaderMap,response::{IntoResponse,Response}};
    use ovg_module_http::{AuthFuture,HttpHost,ModuleHttpState,ModuleSession,SessionProof};
    use ovg_core::AppError;
    use uuid::Uuid;
    use std::sync::Arc;
    use crate::{catalog::CatalogError,identity::{self,HumanSession}};
    struct Proof { session:HumanSession, state:AppState }
    impl SessionProof for Proof {
        fn actor_id(&self)->Uuid{self.session.actor_id()}
        fn session_id(&self)->Uuid{self.session.session_id()}
        fn display_name(&self)->&str{self.session.display_name()}
        fn require_csrf(&self,headers:&HeaderMap)->Result<(),Response>{
            identity::require_csrf(&self.state,&self.session,headers).map_err(IntoResponse::into_response)
        }
    }
    struct Host(AppState);
    impl HttpHost for Host {
        fn authenticate<'a>(&'a self,headers:&'a HeaderMap)->AuthFuture<'a>{
            Box::pin(async move{
                let session=identity::authenticate_cookie_session(&self.0,headers).await.map_err(IntoResponse::into_response)?;
                Ok(ModuleSession::from_verified(Proof{session,state:self.0.clone()}))
            })
        }
        fn project_error(&self,error:AppError)->Response{CatalogError::spatial(error).into_response()}
    }
    pub(super) fn routes(state:&AppState)->Router<AppState>{
        ovg_module_house_numbering_http::router().with_state(ModuleHttpState{
            db_pool:state.db_pool.clone(),modules:state.modules.clone(),
            artifact_store_dir:state.config.artifact_store_dir.clone(),
            host:Arc::new(Host(state.clone())),
        })
    }
}
pub(crate) fn routes(_state:&AppState)->Router<AppState>{
    #[cfg(feature="house-numbering")]
    {return enabled::routes(_state);}
    #[cfg(not(feature="house-numbering"))]
    {Router::new()}
}
