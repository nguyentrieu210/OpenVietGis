use ovg_local_storage::{LocalStore,editing::Edit,Error};
use ovg_mobile_engine::{edit,attributes::{Field,Kind},survey::Fix};
use serde_json::{Value,json};
use uuid::Uuid;
fn temp()->std::path::PathBuf{let d=std::env::temp_dir().join(format!("ovg-w3-{}",Uuid::new_v4()));std::fs::create_dir(&d).unwrap();d}
#[test]
fn persisted_edit_undo_redo_schema_and_atomic_failure(){
 let d=temp();let p=d.join("work.gpkg");let s=LocalStore::open(&p,64*1024*1024).unwrap();
 let schema=[Field{name:"name".into(),kind:Kind::Text,required:true,max_length:Some(20),min:None,max:None}];
 s.set_attribute_schema(&schema).unwrap();let id=Uuid::new_v4();let cmd=Uuid::new_v4();
 let e=Edit{feature_id:id,expected_revision:0,wkb:Some(edit::draft("point",&[106.,10.]).unwrap()),attributes:json!({"name":"Điểm A"})};
 assert!(s.feature_json(id).is_err());
 let mut bad=e.clone();bad.feature_id=Uuid::new_v4();bad.attributes=json!({"name":1});
 assert!(s.commit_edit(cmd,&[e.clone(),bad]).is_err());assert!(s.feature_json(id).is_err());
 s.commit_edit(cmd,&[e.clone()]).unwrap();
 assert!(matches!(s.commit_edit(cmd,&[e.clone()]),Err(Error::Reused)));
 assert!(s.history_step(false).unwrap());assert!(s.feature_json(id).is_err());drop(s);
 let s=LocalStore::open(&p,64*1024*1024).unwrap();
 assert!(s.history_step(true).unwrap());
 let v:Value=serde_json::from_str(&s.feature_json(id).unwrap()).unwrap();assert_eq!(v["revision"],3);
 let hit:Value=serde_json::from_str(&s.snap_local(106.,10.,5.).unwrap()).unwrap();assert_eq!(hit["feature_id"],id.to_string());
 assert!(s.attribute_page(0,100,"Điểm").unwrap().contains("Điểm A"));
 assert!(s.history_step(false).unwrap());
 let mut n=e;n.feature_id=Uuid::new_v4();s.commit_edit(Uuid::new_v4(),&[n]).unwrap();
 assert!(!s.history_step(true).unwrap());drop(s);std::fs::remove_dir_all(d).unwrap();
}
#[test]
fn track_segments_recover_and_gpx(){
 let d=temp();let p=d.join("work.gpkg");let s=LocalStore::open(&p,64*1024*1024).unwrap();
 let t=s.create_track("A & B").unwrap();s.track_state(t,"resume").unwrap();
 let mut f=Fix{lon:106.,lat:10.,accuracy_m:Some(3.),altitude_m:Some(12.),utc_ms:1700000000000,provider:"os".into(),quality:"unknown".into(),satellites:None};
 assert_eq!(s.append_fix(t,&f).unwrap(),1);assert!(s.append_fix(t,&f).is_err());drop(s);
 let s=LocalStore::open(&p,64*1024*1024).unwrap();assert_eq!(s.recover_tracks().unwrap(),1);
 f.utc_ms+=1000;assert!(s.append_fix(t,&f).is_err());s.track_state(t,"resume").unwrap();assert_eq!(s.append_fix(t,&f).unwrap(),2);
 s.track_state(t,"finish").unwrap();assert!(s.append_fix(t,&f).is_err());let g=s.track_gpx(t).unwrap();
 assert_eq!(g.matches("<trkseg>").count(),2);assert!(g.contains("A &amp; B"));assert!(g.contains("2023-11-14T22:13:20.000Z"));drop(s);std::fs::remove_dir_all(d).unwrap();
}
