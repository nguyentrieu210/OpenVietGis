//! Only an isolated fixture may enable the candidate route family.
use axum::body::Body;
use http::{Request, StatusCode};
use openvietgis_api_rust::{build_router, AppConfig, AppState};
use tower::ServiceExt;
#[cfg(feature="mobile-auth-candidate")]
use serde_json::{json, Value};

#[cfg(not(feature="mobile-auth-candidate"))]
#[tokio::test]
async fn default_build_has_no_mobile_identity_routes() {
    let app=build_router(AppState::new(AppConfig::default(),None).unwrap());
    let response=app.oneshot(Request::builder().uri("/api/v1/mobile/auth/session").body(Body::empty()).unwrap()).await.unwrap();
    assert_eq!(response.status(),StatusCode::NOT_FOUND);
}

#[cfg(feature="mobile-auth-candidate")]
#[tokio::test]
async fn isolated_http_grant_exchange_rotation_revocation() {
    let db=std::env::var("OVG_MOBILE_IDENTITY_TEST_DATABASE_URL").expect("isolated fixture required");
    let pool=ovg_storage::DbPool::connect(&db,ovg_storage::DbPoolConfig::default()).await.unwrap();
    let mut cfg=AppConfig::default();
    cfg.api_public_base_url="https://gis.example.invalid".into();
    cfg.session_cookie_secure=true;
    cfg.csrf_secret=Some("fixture-csrf-secret".into());
    let app=build_router(AppState::new(cfg,Some(pool)).unwrap());
    let cookie="ovg_session=ovgs_11111111-1111-4111-8111-111111111111.fixture-secret";
    async fn body(res:axum::response::Response)->Value{
        let b=axum::body::to_bytes(res.into_body(),8192).await.unwrap();
        serde_json::from_slice(&b).unwrap()
    }
    let res=app.clone().oneshot(Request::builder().uri("/api/v1/auth/csrf").header("cookie",cookie).body(Body::empty()).unwrap()).await.unwrap();
    assert_eq!(res.status(),StatusCode::OK);
    let csrf=body(res).await["token"].as_str().unwrap().to_owned();
    let grant=json!({"client_id":"openvietgis-mobile","redirect_uri":"https://gis.example.invalid/mobile/callback",
        "code_challenge":"E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM","code_challenge_method":"S256",
        "device_id":"44444444-4444-4444-8444-444444444444","consent":true});
    let grant_req=|csrf: &str|Request::builder().method("POST").uri("/api/v1/mobile/auth/grant").header("cookie",cookie)
        .header("x-openvietgis-csrf",csrf).header("content-type","application/json").body(Body::from(grant.to_string())).unwrap();
    let res=app.clone().oneshot(grant_req("wrong")).await.unwrap();assert_eq!(res.status(),StatusCode::FORBIDDEN);
    let res=app.clone().oneshot(grant_req(&csrf)).await.unwrap();assert_eq!(res.status(),StatusCode::OK);
    assert_eq!(res.headers()["cache-control"],"private, no-store");
    let code=body(res).await["code"].as_str().unwrap().to_owned();
    let payload=json!({"client_id":"openvietgis-mobile","redirect_uri":"https://gis.example.invalid/mobile/callback",
        "code":code,"code_verifier":"dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk"});
    let req=|path:&str, value:&Value|Request::builder().method("POST").uri(path).header("content-type","application/json").body(Body::from(value.to_string())).unwrap();
    let res=app.clone().oneshot(req("/api/v1/mobile/auth/exchange",&payload)).await.unwrap();assert_eq!(res.status(),StatusCode::OK);
    let a=body(res).await;
    assert_eq!(a["sub"],"22222222-2222-4222-8222-222222222222");
    let res=app.clone().oneshot(req("/api/v1/mobile/auth/exchange",&payload)).await.unwrap();assert_eq!(res.status(),StatusCode::UNAUTHORIZED);
    let bearer=format!("Bearer {}",a["access_token"].as_str().unwrap());
    let res=app.clone().oneshot(Request::builder().uri("/api/v1/mobile/auth/session").header("authorization",&bearer).header("cookie",cookie).body(Body::empty()).unwrap()).await.unwrap();assert_eq!(res.status(),StatusCode::UNAUTHORIZED);
    let res=app.clone().oneshot(Request::builder().uri("/api/v1/mobile/auth/session").header("authorization",&bearer).body(Body::empty()).unwrap()).await.unwrap();assert_eq!(res.status(),StatusCode::OK);
    let res=app.clone().oneshot(Request::builder().uri("/api/v1/me").header("authorization",&bearer).body(Body::empty()).unwrap()).await.unwrap();assert_eq!(res.status(),StatusCode::UNAUTHORIZED);
    let refresh=json!({"client_id":"openvietgis-mobile","refresh_token":a["refresh_token"]});
    let res=app.clone().oneshot(req("/api/v1/mobile/auth/refresh",&refresh)).await.unwrap();assert_eq!(res.status(),StatusCode::OK);
    let b=body(res).await;assert_ne!(a["access_token"],b["access_token"]);
    let res=app.clone().oneshot(Request::builder().method("POST").uri("/api/v1/mobile/auth/revoke").header("authorization",format!("Bearer {}",b["access_token"].as_str().unwrap())).body(Body::empty()).unwrap()).await.unwrap();assert_eq!(res.status(),StatusCode::OK);
    let res=app.oneshot(Request::builder().uri("/api/v1/mobile/auth/session").header("authorization",format!("Bearer {}",b["access_token"].as_str().unwrap())).body(Body::empty()).unwrap()).await.unwrap();assert_eq!(res.status(),StatusCode::UNAUTHORIZED);
}
