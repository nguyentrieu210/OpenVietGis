//! Recovery and integration tests for the standalone shared store.
use crate::host::Host;
use ovg_local_storage::{LocalStore, Mutation};
use serde_json::json;
use uuid::Uuid;
use std::path::PathBuf;
fn point(x:f64,y:f64)->Vec<u8>{ovg_mobile_engine::edit::draft("point",&[x,y]).unwrap()}
#[test]
fn child_commits_then_exits_without_drop(){
 let Some(root)=std::env::var_os("OVG_CRASH_TEST_ROOT")else{return};
 let store=LocalStore::open(&PathBuf::from(root).join("crash.gpkg"),8*1024*1024).unwrap();
 store.apply(&Mutation{operation_id:Uuid::new_v4(),feature_id:Uuid::from_u128(1),expected_revision:0,wkb:Some(point(106.,11.)),attributes:json!({"recovery":true})}).unwrap();
 // Deliberately bypass Rust destructors, including LocalStore/SQLite close.
 std::process::exit(77)
}
#[test]
fn committed_edit_survives_process_exit(){
 let dir=tempfile::tempdir().unwrap();
 let status=std::process::Command::new(std::env::current_exe().unwrap())
   .args(["--exact","recovery_tests::child_commits_then_exits_without_drop","--nocapture"])
   .env("OVG_CRASH_TEST_ROOT",dir.path()).status().unwrap();
 assert_eq!(status.code(),Some(77));
 let store=LocalStore::open(&dir.path().join("crash.gpkg"),8*1024*1024).unwrap();
 let rows=store.query([105.,10.,107.,12.],0,2).unwrap();
 assert_eq!(rows.len(),1);assert_eq!(rows[0].1.attributes["recovery"],true);
}
#[test]
fn corrupt_file_fails_closed(){
 let dir=tempfile::tempdir().unwrap();let path=dir.path().join("bad.gpkg");
 std::fs::write(&path,b"not SQLite").unwrap();
 assert!(LocalStore::open(&path,4*1024*1024).is_err());
 assert_eq!(std::fs::read(path).unwrap(),b"not SQLite");
}
#[test]
fn quota_failure_does_not_commit_partial_feature(){
 let dir=tempfile::tempdir().unwrap();let store=LocalStore::open(&dir.path().join("quota.gpkg"),4*1024*1024).unwrap();
 let mut committed=0;let mut refused=false;
 for _ in 0..128{
   let id=Uuid::new_v4();let m=Mutation{operation_id:Uuid::new_v4(),feature_id:id,expected_revision:0,wkb:Some(point(106.,11.)),attributes:json!({"payload":"x".repeat(120_000)})};
   match store.apply(&m){Ok(_)=>committed+=1,Err(_)=>{assert!(store.feature_json(id).is_err());refused=true;break}}
 }
 assert!(refused);assert!(committed>0);
 let mut after=0;let mut rows=0;
 loop{let page=store.query([105.,10.,107.,12.],after,10).unwrap();if page.is_empty(){break}after=page.last().unwrap().0;rows+=page.len();}
 assert_eq!(rows,committed);
}
#[test]
fn mvt_revision_cache_and_workspace_isolation(){
 let dir=tempfile::tempdir().unwrap();let h=Host::new(dir.path().to_path_buf()).unwrap();let p=h.create().unwrap();let w=h.open(p.id).unwrap();let id=Uuid::new_v4();
 h.edit(w.handle,Uuid::new_v4(),id,"0".into(),Some("point".into()),vec![106.,11.],json!({})).unwrap();
 let tile=h.vector_tile(w.handle,w.handle,1,0,0,0).unwrap();
 assert_eq!(tile,h.vector_tile(w.handle,w.handle,1,0,0,0).unwrap());
 assert!(h.vector_tile(w.handle,Uuid::new_v4(),1,0,0,0).is_err());
 let p=h.attributes(w.handle,"0".into(),100,"".into()).unwrap();
 let cursor=p["next"].as_str().unwrap().to_string();
 h.reshape(w.handle,id,"1".into(),json!({"type":"Point","coordinates":[107.,12.]})).unwrap();
 assert!(h.vector_tile(w.handle,w.handle,1,0,0,0).is_err());
 assert!(h.attributes(w.handle,cursor,100,"".into()).is_err());
 assert_ne!(tile,h.vector_tile(w.handle,w.handle,2,0,0,0).unwrap());
 h.close(w.handle).unwrap();assert!(h.vector_tile(w.handle,w.handle,2,0,0,0).is_err());
}
#[test]
fn standard_external_gpkg_import_roundtrip(){
 use rusqlite::{Connection,params};
 let dir=tempfile::tempdir().unwrap();let src=dir.path().join("external.gpkg");
 let db=Connection::open(&src).unwrap();
 db.execute_batch("PRAGMA application_id=1196444487; CREATE TABLE gpkg_spatial_ref_sys(srs_id INTEGER,organization TEXT,organization_coordsys_id INTEGER); INSERT INTO gpkg_spatial_ref_sys VALUES(4326,'EPSG',4326),(3857,'EPSG',3857); CREATE TABLE gpkg_contents(table_name TEXT,data_type TEXT); CREATE TABLE gpkg_geometry_columns(table_name TEXT,column_name TEXT,srs_id INTEGER,z INTEGER,m INTEGER); CREATE TABLE survey(fid INTEGER PRIMARY KEY,geom BLOB,name TEXT); INSERT INTO gpkg_contents VALUES('survey','features'); INSERT INTO gpkg_geometry_columns VALUES('survey','geom',4326,0,0);").unwrap();
 let mut blob=vec![b'G',b'P',0,1];blob.extend(4326_i32.to_le_bytes());blob.extend(point(106.,11.));
 db.execute("INSERT INTO survey VALUES(1,?1,'sample')",params![blob]).unwrap();drop(db);
 let target=LocalStore::open(&dir.path().join("target.gpkg"),8*1024*1024).unwrap();
 assert_eq!(crate::gpkg_import::import(&target,&src).unwrap(),1);
 let rows=target.query([105.,10.,107.,12.],0,2).unwrap();assert_eq!(rows.len(),1);assert_eq!(rows[0].1.attributes["name"],"sample");
}

#[test]
fn restore_snapshot_preserves_revision_and_undo_history(){
 let dir=tempfile::tempdir().unwrap();let h=Host::new(dir.path().join("projects")).unwrap();
 let p=h.create().unwrap();let w=h.open(p.id).unwrap();let id=Uuid::new_v4();
 h.edit(w.handle,Uuid::new_v4(),id,"0".into(),Some("point".into()),vec![106.,11.],json!({"name":"before"})).unwrap();
 h.update_attributes(w.handle,id,"1".into(),json!({"name":"after"})).unwrap();
 let snap=dir.path().join("backup.gpkg");h.snapshot(w.handle,&snap).unwrap();
 let restored=h.restore_snapshot(&snap).unwrap();assert_ne!(restored.id,p.id);
 let r=h.open(restored.id).unwrap();assert_eq!(h.feature(r.handle,id).unwrap()["revision"],"2");
 assert!(h.history(r.handle,false).unwrap());assert_eq!(h.feature(r.handle,id).unwrap()["attributes"]["name"],"before");
 assert_eq!(h.feature(w.handle,id).unwrap()["attributes"]["name"],"after");
 h.close(r.handle).unwrap();h.close(w.handle).unwrap();
}
#[test]
fn rejects_snapshot_with_untrusted_trigger(){
 let dir=tempfile::tempdir().unwrap();let h=Host::new(dir.path().join("projects")).unwrap();
 let p=h.create().unwrap();let w=h.open(p.id).unwrap();let snap=dir.path().join("backup.gpkg");h.snapshot(w.handle,&snap).unwrap();
 let db=rusqlite::Connection::open(&snap).unwrap();
 db.execute_batch("CREATE TRIGGER unexpected AFTER INSERT ON features BEGIN DELETE FROM features; END;").unwrap();drop(db);
 assert!(h.restore_snapshot(&snap).is_err());assert_eq!(h.list().unwrap().len(),1);
}

#[test]
fn named_layers_persist_and_remain_isolated(){
 let d=tempfile::tempdir().unwrap();let h=Host::new(d.path().to_path_buf()).unwrap();
 let p=h.create().unwrap();let w=h.open(p.id).unwrap();let layer=h.create_layer(w.handle,"Survey".into(),None).unwrap();
 let child=h.open_layer(w.handle,layer.id).unwrap();let id=Uuid::new_v4();
 h.edit(child.handle,Uuid::new_v4(),id,"0".into(),Some("point".into()),vec![106.,11.],json!({"layer":"survey"})).unwrap();
 assert!(h.feature(w.handle,id).is_err());assert_eq!(h.list_layers(w.handle).unwrap().len(),1);
 h.close(w.handle).unwrap();assert!(h.feature(child.handle,id).is_err());
 let w=h.open(p.id).unwrap();assert_eq!(h.list_layers(w.handle).unwrap()[0].name,"Survey");
 let child=h.open_layer(w.handle,layer.id).unwrap();assert_eq!(h.feature(child.handle,id).unwrap()["attributes"]["layer"],"survey");
 h.close(w.handle).unwrap();
}
#[test]
fn multi_table_catalog_imports_only_selected_layer_and_reprojects(){
 use rusqlite::{Connection,params};
 let d=tempfile::tempdir().unwrap();let src=d.path().join("multi.gpkg");
 let db=Connection::open(&src).unwrap();
 db.execute_batch("PRAGMA application_id=1196444487; CREATE TABLE gpkg_spatial_ref_sys(srs_id INTEGER,organization TEXT,organization_coordsys_id INTEGER); INSERT INTO gpkg_spatial_ref_sys VALUES(4326,'EPSG',4326),(3857,'EPSG',3857); CREATE TABLE gpkg_contents(table_name TEXT,data_type TEXT); CREATE TABLE gpkg_geometry_columns(table_name TEXT,column_name TEXT,srs_id INTEGER,z INTEGER,m INTEGER); CREATE TABLE a(fid INTEGER PRIMARY KEY,geom BLOB,name TEXT); CREATE TABLE b(fid INTEGER PRIMARY KEY,geom BLOB,name TEXT); INSERT INTO gpkg_contents VALUES('a','features'),('b','features'); INSERT INTO gpkg_geometry_columns VALUES('a','geom',4326,0,0),('b','geom',3857,0,0);").unwrap();
 let mut blob=vec![b'G',b'P',0,1];blob.extend(3857_i32.to_le_bytes());
 let mut point_wkb=vec![1,1,0,0,0];point_wkb.extend(111319.49079327357_f64.to_le_bytes());point_wkb.extend(0_f64.to_le_bytes());blob.extend(point_wkb);
 db.execute("INSERT INTO b VALUES(1,?1,'mercator')",params![blob]).unwrap();drop(db);
 let h=Host::new(d.path().join("projects")).unwrap();let p=h.create().unwrap();let w=h.open(p.id).unwrap();
 let c=h.inspect_gpkg(w.handle,&src).unwrap();assert_eq!(c["layers"].as_array().unwrap().len(),2);
 let grant=Uuid::parse_str(c["id"].as_str().unwrap()).unwrap();
 let layer=h.create_layer(w.handle,"B".into(),Some((grant,"b".into()))).unwrap();
 assert_eq!(layer.source_epsg,Some(3857));
 let child=h.open_layer(w.handle,layer.id).unwrap();
 let v=h.viewport(child.handle,[0.9,-0.1,1.1,0.1]).unwrap();assert_eq!(v.data["features"].as_array().unwrap().len(),1);
 assert!(h.viewport(w.handle,[0.9,-0.1,1.1,0.1]).unwrap().data["features"].as_array().unwrap().is_empty());
 h.close(w.handle).unwrap();
}

#[test]
fn complete_project_bundle_preserves_root_and_layer_journals(){
 let d=tempfile::tempdir().unwrap();let h=Host::new(d.path().join("projects")).unwrap();
 let p=h.create().unwrap();let w=h.open(p.id).unwrap();
 let l=h.create_layer(w.handle,"survey".into(),None).unwrap();let child=h.open_layer(w.handle,l.id).unwrap();
 let root_id=Uuid::new_v4();let child_id=Uuid::new_v4();
 h.edit(w.handle,Uuid::new_v4(),root_id,"0".into(),Some("point".into()),vec![106.,11.],json!({})).unwrap();
 h.edit(child.handle,Uuid::new_v4(),child_id,"0".into(),Some("point".into()),vec![107.,12.],json!({"a":1})).unwrap();
 h.update_attributes(child.handle,child_id,"1".into(),json!({"a":2})).unwrap();
 let file=d.path().join("project.ovgproject");h.backup_project(w.handle,&file).unwrap();
 assert!(h.backup_project(w.handle,&file).is_err());
 let p2=h.restore_project(&file).unwrap();assert_ne!(p.id,p2.id);
 let w2=h.open(p2.id).unwrap();assert_eq!(h.list_layers(w2.handle).unwrap()[0].id,l.id);
 let c2=h.open_layer(w2.handle,l.id).unwrap();
 assert_eq!(h.feature(c2.handle,child_id).unwrap()["revision"],"2");
 assert!(h.history(c2.handle,false).unwrap());assert_eq!(h.feature(c2.handle,child_id).unwrap()["attributes"]["a"],1);
 assert!(h.feature(w2.handle,root_id).is_ok());
 assert_eq!(h.feature(child.handle,child_id).unwrap()["attributes"]["a"],2);
 h.close(w.handle).unwrap();h.close(w2.handle).unwrap();
}
