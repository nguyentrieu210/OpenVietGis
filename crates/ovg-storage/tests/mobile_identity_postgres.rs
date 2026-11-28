//! Requires explicitly isolated fixture PostgreSQL database, restricted runtime role.
use ovg_storage::{DbPool,DbPoolConfig,identity,mobile_identity as mobile};
use uuid::Uuid;
#[tokio::test]
async fn grant_rotation_reuse_and_revocation(){
 let url=std::env::var("OVG_MOBILE_IDENTITY_TEST_DATABASE_URL").expect("isolated test DB required");
 let pool=DbPool::connect(&url,DbPoolConfig::default()).await.unwrap();
 let browser="ovgs_11111111-1111-4111-8111-111111111111.fixture-secret";
 let h=identity::authenticate_session(&pool,browser).await.ok().expect("fixture web identity");
 let client=mobile::Client::reviewed("https://gis.example.invalid","mobile","https://mobile.example.invalid/callback").unwrap();
 let wrong=mobile::Client::reviewed("https://other.example.invalid","mobile","https://mobile.example.invalid/callback").unwrap();
 let verifier="dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk";
 let challenge="E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM";
 let redirect="https://mobile.example.invalid/callback";
 let device=Uuid::new_v4();
 assert!(mobile::issue_grant(&pool,&h,&client,"https://evil.invalid",challenge,device).await.is_err());
 let code=mobile::issue_grant(&pool,&h,&client,redirect,challenge,device).await.unwrap();
 assert!(mobile::exchange(&pool,&wrong,&code,redirect,verifier).await.is_err());
 assert!(mobile::exchange(&pool,&client,&code,redirect,"wrong").await.is_err());
 let a=mobile::exchange(&pool,&client,&code,redirect,verifier).await.unwrap();
 assert!(mobile::exchange(&pool,&client,&code,redirect,verifier).await.is_err());
 let actor=mobile::authenticate(&pool,&client,&a.access).await.unwrap();
 assert_eq!(actor.user_id,h.user_id());assert_eq!(actor.device_id,device);
 assert!(mobile::authenticate(&pool,&wrong,&a.access).await.is_err());
 assert!(mobile::authenticate(&pool,&client,browser).await.is_err());
 assert!(mobile::authenticate(&pool,&client,&a.refresh).await.is_err());
 assert!(identity::authenticate_session(&pool,&a.access).await.is_err());
 let b=mobile::rotate(&pool,&client,&a.refresh).await.unwrap();
 assert!(mobile::authenticate(&pool,&client,&a.access).await.is_err());
 assert!(mobile::authenticate(&pool,&client,&b.access).await.is_ok());
 assert!(matches!(mobile::rotate(&pool,&client,&a.refresh).await,Err(mobile::Error::Revoked)));
 assert!(mobile::authenticate(&pool,&client,&b.access).await.is_err());
 assert!(mobile::rotate(&pool,&client,&b.refresh).await.is_err());
 let code=mobile::issue_grant(&pool,&h,&client,redirect,challenge,device).await.unwrap();
 let c=mobile::exchange(&pool,&client,&code,redirect,verifier).await.unwrap();
 let (x,y)=tokio::join!(mobile::rotate(&pool,&client,&c.refresh),mobile::rotate(&pool,&client,&c.refresh));
 assert!(x.is_ok()^y.is_ok());
 let winner=x.or(y).unwrap();assert!(mobile::authenticate(&pool,&client,&winner.access).await.is_err());
 let code=mobile::issue_grant(&pool,&h,&client,redirect,challenge,device).await.unwrap();
 let d=mobile::exchange(&pool,&client,&code,redirect,verifier).await.unwrap();
 let actor=mobile::authenticate(&pool,&client,&d.access).await.unwrap();mobile::revoke(&pool,&actor).await.unwrap();
 assert!(mobile::authenticate(&pool,&client,&d.access).await.is_err());
 assert!(identity::authenticate_session(&pool,browser).await.is_ok());
}
