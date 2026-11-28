use ovg_local_storage::{LocalStore,Mutation};
use uuid::Uuid;
use serde_json::{json,Value};
#[test]fn search_and_revision_pinned_processing_input(){
 let dir=std::env::temp_dir().join(format!("ovg-w6-{}",Uuid::new_v4()));std::fs::create_dir(&dir).unwrap();
 let s=LocalStore::open(&dir.join("work.gpkg"),64*1024*1024).unwrap();
 let id=Uuid::new_v4();let wkb=ovg_mobile_engine::edit::draft("point",&[106.,10.]).unwrap();
 s.apply(&Mutation{operation_id:Uuid::new_v4(),feature_id:id,expected_revision:0,wkb:Some(wkb.clone()),attributes:json!({"name":"Điểm khảo sát"})}).unwrap();
 let hits:Value=serde_json::from_str(&s.search_features("khảo sát",0,10).unwrap()).unwrap();
 assert_eq!(hits[0]["id"],id.to_string());assert_eq!(hits[0]["bbox"],json!([106.,10.,106.,10.]));
 let(a,b)=s.processing_inputs(&[id],&[]).unwrap();assert_eq!(a[0].id,format!("{id}@1"));assert!(b.is_empty());assert_eq!(a[0].wkb,wkb);
 assert!(s.processing_inputs(&[id,id],&[]).is_err());assert!(s.processing_inputs(&[Uuid::new_v4()],&[]).is_err());
 drop(s);std::fs::remove_dir_all(dir).unwrap();
}
