use ovg_project_store::{Store,Batch,Edit,Image,Error,migration::{Source,migrate},virtual_query::{Plan,Filter}};
use ovg_local_storage::{LocalStore,Mutation,editing::Edit as LegacyEdit};
use serde_json::json;
use std::sync::atomic::AtomicBool;
use uuid::Uuid;
fn point(x:f64)->Vec<u8>{ovg_mobile_engine::edit::draft("point",&[x,11.]).unwrap()}
#[test]
fn migrate_live_tombstone_and_history_without_mutating_sources(){
 let d=tempfile::tempdir().unwrap();let a=Uuid::new_v4();let b=Uuid::new_v4();
 let f=Uuid::new_v4();let gone=Uuid::new_v4();let second=Uuid::new_v4();
 let source=d.path().join("a.gpkg");let snap=d.path().join("a-copy.gpkg");
 {let s=LocalStore::open(&source,32*1024*1024).unwrap();
  s.apply(&Mutation{operation_id:Uuid::new_v4(),feature_id:gone,expected_revision:0,wkb:Some(point(106.)),attributes:json!({})}).unwrap();
  s.apply(&Mutation{operation_id:Uuid::new_v4(),feature_id:gone,expected_revision:1,wkb:None,attributes:json!({})}).unwrap();
  s.commit_edit(Uuid::new_v4(),&[LegacyEdit{feature_id:f,expected_revision:0,wkb:Some(point(107.)),attributes:json!({"name":"A"})}]).unwrap();
  s.update_attributes(f,1,json!({"name":"changed"})).unwrap();
  s.export_snapshot(&snap).unwrap();
 }
 let src2=d.path().join("b.gpkg");let snap2=d.path().join("b-copy.gpkg");
 {let s=LocalStore::open(&src2,32*1024*1024).unwrap();s.apply(&Mutation{operation_id:Uuid::new_v4(),feature_id:second,expected_revision:0,wkb:Some(point(108.)),attributes:json!({})}).unwrap();s.export_snapshot(&snap2).unwrap();}
 let bytes=std::fs::read(&snap).unwrap();let output=d.path().join("new.sqlite");
 let inputs=[Source{layer:a,name:"A".into(),snapshot:snap.clone()},Source{layer:b,name:"B".into(),snapshot:snap2}];
 let report=migrate(&inputs,&output,&AtomicBool::new(false)).unwrap();
 assert_eq!(report[0].live,1);assert_eq!(report[0].tombstones,1);assert_eq!(report[0].journal,4);assert_eq!(report[0].history,2);
 assert_eq!(std::fs::read(&snap).unwrap(),bytes);
 assert!(migrate(&inputs,&output,&AtomicBool::new(false)).is_err());
 let s=Store::reopen(&output).unwrap();assert!(s.integrity().unwrap());
 assert_eq!(s.query(a,[0.,0.,180.,90.],0,10,1).unwrap().rows[0].revision,2);
 assert_eq!(s.report().unwrap().tombstones,1);
 let stale=Batch{operation:Uuid::new_v4(),expected_project:1,edits:vec![Edit{layer:a,feature:gone,expected:0,image:Some(Image{wkb:point(106.),attributes:json!({})})}],links:vec![]};
 assert_eq!(s.apply(&stale),Err(Error::Conflict));
 let revived=Batch{edits:vec![Edit{expected:2,..stale.edits[0].clone()}],..stale};assert_eq!(s.apply(&revived).unwrap().features[0].2,3);
 let cancelled=d.path().join("cancelled.sqlite");
 assert!(migrate(&inputs,&cancelled,&AtomicBool::new(true)).is_err());assert!(!cancelled.exists());
}
#[test]
fn virtual_filters_and_maintenance_preserve_data(){
 let d=tempfile::tempdir().unwrap();let p=d.path().join("p.sqlite");let s=Store::create(&p).unwrap();let l=Uuid::new_v4();
 s.add_layer(l,"Survey",0).unwrap();
 let ids:[Uuid;3]=std::array::from_fn(|_|Uuid::new_v4());
 s.apply(&Batch{operation:Uuid::new_v4(),expected_project:1,edits:ids.iter().enumerate().map(|(i,id)|Edit{layer:l,feature:*id,expected:0,image:Some(Image{wkb:point(106.+i as f64),attributes:json!({"name":"A","value":i})})}).collect(),links:vec![]}).unwrap();
 let plan=Plan{layer:l,bbox:[0.,0.,180.,90.],filters:vec![Filter::NumberRange{field:"value".into(),min:1.,max:2.}],relation:None};
 let page=s.virtual_page(&plan,None,1,2).unwrap();assert_eq!(page.rows[0].id,ids[1]);
 let page2=s.virtual_page(&plan,page.next.as_ref(),1,2).unwrap();assert_eq!(page2.rows[0].id,ids[2]);
 let mut wrong=plan.clone();wrong.filters.clear();assert!(s.virtual_page(&wrong,page.next.as_ref(),1,2).is_err());
 assert!(s.rebuild_index(2,&AtomicBool::new(true)).is_err());assert_eq!(s.report().unwrap().features,3);
 assert_eq!(s.rebuild_index(2,&AtomicBool::new(false)).unwrap(),3);
 let copy=d.path().join("compact.sqlite");s.compact_copy(&copy,2).unwrap();
 assert!(s.compact_copy(&copy,2).is_err());
 let c=Store::reopen(&copy).unwrap();assert!(c.integrity().unwrap());assert_eq!(c.report().unwrap().features,3);
}
