//! Module-owned Axum route adapter. The Core composition root supplies a verified host.
mod handlers;
pub use handlers::router;

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{body::Body,http::{HeaderMap,Request,StatusCode},response::{IntoResponse,Response},Json};
    use ovg_core::AppError;
    use ovg_module_http::{AuthFuture,HttpHost,ModuleHttpState,ModuleSession,SessionProof};
    use ovg_module_sdk::ModuleRegistry;
    use serde_json::{json,Value};
    use std::{path::PathBuf,sync::Arc};
    use tower::ServiceExt;
    use uuid::Uuid;

    struct FixtureSession;
    impl SessionProof for FixtureSession {
        fn actor_id(&self)->Uuid{Uuid::from_u128(1)}
        fn session_id(&self)->Uuid{Uuid::from_u128(2)}
        fn display_name(&self)->&str{"Fixture"}
        fn require_csrf(&self,headers:&HeaderMap)->Result<(),Response>{
            if headers.get("x-openvietgis-csrf").is_some_and(|v|v=="fixture"){Ok(())}
            else{Err((StatusCode::FORBIDDEN,Json(json!({"detail":"CSRF validation failed"}))).into_response())}
        }
    }
    struct FixtureHost;
    impl HttpHost for FixtureHost {
        fn authenticate<'a>(&'a self,headers:&'a HeaderMap)->AuthFuture<'a>{
            Box::pin(async move{
                if headers.get("cookie").is_some_and(|v|v=="fixture") {
                    Ok(ModuleSession::from_verified(FixtureSession))
                }else{
                    Err((StatusCode::UNAUTHORIZED,Json(json!({"detail":"authentication required"}))).into_response())
                }
            })
        }
        fn project_error(&self,error:AppError)->Response{
            (StatusCode::from_u16(error.status_code()).unwrap(),Json(json!({"detail":error.to_string()}))).into_response()
        }
    }
    #[tokio::test]
    async fn module_router_uses_host_auth_csrf_and_project_error_contract(){
        let app=router().with_state(ModuleHttpState{db_pool:None,modules:ModuleRegistry::new(),artifact_store_dir:PathBuf::new(),host:Arc::new(FixtureHost)});
        let url=format!("/organizations/{}/projects/{}/modules/house-numbering/rules",Uuid::from_u128(3),Uuid::from_u128(4));
        let response=app.clone().oneshot(Request::builder().uri(&url).body(Body::empty()).unwrap()).await.unwrap();
        assert_eq!(response.status(),StatusCode::UNAUTHORIZED);
        let body:Value=serde_json::from_slice(&axum::body::to_bytes(response.into_body(),4096).await.unwrap()).unwrap();
        assert_eq!(body["detail"],"authentication required");
        let request=||Request::builder().method("POST").uri(&url).header("cookie","fixture").header("content-type","application/json");
        let response=app.clone().oneshot(request().body(Body::from(r#"{"version":"draft"}"#)).unwrap()).await.unwrap();
        assert_eq!(response.status(),StatusCode::FORBIDDEN);
        let response=app.oneshot(request().header("x-openvietgis-csrf","fixture").body(Body::from(r#"{"version":"draft"}"#)).unwrap()).await.unwrap();
        assert_eq!(response.status(),StatusCode::SERVICE_UNAVAILABLE);
    }
}
