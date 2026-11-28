use ovg_local_storage::{LocalStore,editing::Edit};
use ovg_mobile_engine::edit;
use serde_json::{Value,json};
use uuid::Uuid;
#[test]
fn split_merge_atomic_and_recover_history(){
 let d=std::env::temp_dir().join(format!("ovg-split-{}",Uuid::new_v4()));std::fs::create_dir(&d).unwrap();
 let p=d.join("work.gpkg");let s=LocalStore::open(&p,64*1024*1024).unwrap();
 let a=Uuid::new_v4();s.commit_edit(Uuid::new_v4(),&[Edit{feature_id:a,expected_revision:0,wkb:Some(edit::draft("line",&[106.,10.,106.01,10.01,106.02,10.02]).unwrap()),attributes:json!({"name":"line"})}]).unwrap();
 let b=s.split_feature(a,1,1).unwrap();assert!(s.split_feature(a,1,1).is_err());
 s.merge_features(a,2,b,1).unwrap();assert!(s.feature_json(b).is_err());
 assert!(s.history_step(false).unwrap());assert!(s.feature_json(b).is_ok());
 let f:Value=serde_json::from_str(&s.feature_json(a).unwrap()).unwrap();let rev=f["revision"].as_u64().unwrap();
 s.update_attributes(a,rev,json!({"name":"new"})).unwrap();assert!(s.update_attributes(a,rev,json!({"name":"stale"})).is_err());
 assert!(!s.history_step(true).unwrap());drop(s);std::fs::remove_dir_all(d).unwrap();
}
