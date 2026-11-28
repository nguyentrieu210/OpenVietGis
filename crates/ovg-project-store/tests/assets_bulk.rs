use ovg_project_store::{Store,Image,Error,bulk_import::BulkRow};
use serde_json::json;
use std::sync::atomic::AtomicBool;
use uuid::Uuid;
fn row(i:u64)->BulkRow{
 let id=Uuid::from_u128(i as u128+1);
 BulkRow{id,image:Image{wkb:ovg_mobile_engine::edit::draft("point",&[106.+(i%100)as f64/1000.,11.]).unwrap(),attributes:json!({"i":i})}}
}
#[test]fn atomic_bulk_replay_and_rollback(){
 let d=tempfile::tempdir().unwrap();let s=Store::create(&d.path().join("p.sqlite")).unwrap();
 let l=Uuid::new_v4();let op=Uuid::new_v4();let cancel=AtomicBool::new(false);
 let bad=(0..1000).map(|i|if i==999{Err(Error::Invalid)}else{Ok(row(i))});
 assert_eq!(s.import_layer_stream(op,l,"survey",0,bad,&cancel),Err(Error::Invalid));
 assert_eq!(s.revision().unwrap(),0);assert!(s.catalog().unwrap().layers.is_empty());
 let r=s.import_layer_stream(op,l,"survey",0,(0..1000).map(|i|Ok(row(i))),&cancel).unwrap();
 assert_eq!(r.count,1000);assert_eq!(r.revision,1);
 assert_eq!(s.import_layer_stream(op,l,"survey",0,(0..1000).map(|i|Ok(row(i))),&cancel).unwrap(),r);
 assert_eq!(s.import_layer_stream(op,l,"survey",0,(0..999).map(|i|Ok(row(i))),&cancel),Err(Error::Reused));
 assert!(s.integrity().unwrap());assert_eq!(s.report().unwrap().features,1000);
 assert!(s.import_layer_stream(Uuid::new_v4(),Uuid::new_v4(),"cancel",1,(0..100).map(|i|Ok(row(i))),&AtomicBool::new(true)).is_err());
 assert_eq!(s.revision().unwrap(),1);
}
#[test]fn assets_roundtrip_backup_detach_and_no_clobber(){
 let d=tempfile::tempdir().unwrap();let s=Store::create(&d.path().join("p.sqlite")).unwrap();
 let data=vec![17u8;2*1024*1024+123];let src=d.path().join("source.bin");std::fs::write(&src,&data).unwrap();
 let id=Uuid::new_v4();let cancel=AtomicBool::new(false);
 let a=s.attach_asset(id,"las",Some(4326),&src,0,&cancel).unwrap();
 assert_eq!(a.bytes,data.len()as u64);assert_eq!(s.revision().unwrap(),1);
 assert_eq!(s.assets().unwrap(),vec![a.clone()]);assert!(s.integrity().unwrap());
 let backup=d.path().join("backup.sqlite");s.backup_copy(&backup,1,&cancel).unwrap();
 let dst=d.path().join("restored.sqlite");assert_eq!(Store::restore_copy(&backup,&dst,&cancel).unwrap(),1);
 let r=Store::reopen(&dst).unwrap();let out=d.path().join("asset.bin");
 assert_eq!(r.materialize_asset(id,&out,&cancel).unwrap(),a);assert_eq!(std::fs::read(&out).unwrap(),data);
 assert!(r.materialize_asset(id,&out,&cancel).is_err());
 assert!(r.detach_asset(id,0).is_err());assert_eq!(r.detach_asset(id,1).unwrap(),2);
 assert!(r.assets().unwrap().is_empty());assert_eq!(s.assets().unwrap().len(),1);
 assert!(r.integrity().unwrap());
}
#[test]fn candidate_v2_additive_upgrade(){
 let d=tempfile::tempdir().unwrap();let p=d.path().join("v2.sqlite");
 {let db=rusqlite::Connection::open(&p).unwrap();
  let (schema,_)=include_str!("../src/schema.sql").split_once("-- V3 additive assets and bulk import").unwrap();
  db.execute_batch(schema).unwrap();db.pragma_update(None,"application_id",0x4f564750i64).unwrap();db.pragma_update(None,"user_version",2).unwrap();}
 let s=Store::reopen(&p).unwrap();assert_eq!(s.revision().unwrap(),0);assert!(s.assets().unwrap().is_empty());assert!(s.integrity().unwrap());drop(s);
 let db=rusqlite::Connection::open(&p).unwrap();let v:i64=db.pragma_query_value(None,"user_version",|r|r.get(0)).unwrap();assert_eq!(v,4);
}
#[test]fn detects_tampered_resource_chunk(){
 let d=tempfile::tempdir().unwrap();let p=d.path().join("p.sqlite");let src=d.path().join("src");std::fs::write(&src,b"original").unwrap();
 let id=Uuid::new_v4();let s=Store::create(&p).unwrap();s.attach_asset(id,"glb",None,&src,0,&AtomicBool::new(false)).unwrap();s.checkpoint().unwrap();drop(s);
 let db=rusqlite::Connection::open(&p).unwrap();db.execute("UPDATE project_asset_chunk SET data=?1",[b"tampered".as_slice()]).unwrap();drop(db);
 let s=Store::reopen(&p).unwrap();assert!(!s.integrity().unwrap());assert!(s.materialize_asset(id,&d.path().join("out"),&AtomicBool::new(false)).is_err());
}
