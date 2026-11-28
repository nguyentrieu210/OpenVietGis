use ovg_project_store::{Store,Error,mobile,migration::{Source,migrate}};
use ovg_mobile_engine::survey::Fix;
use ovg_local_storage::LocalStore;
use std::sync::atomic::AtomicBool;
use uuid::Uuid;
use serde_json::{Value,json};

#[test]
fn survey_snapshot_retains_fix_quality_and_segments(){
 let d=tempfile::tempdir().unwrap();let p=d.path().join("legacy.gpkg");
 let legacy=LocalStore::open(&p,512*1024*1024).unwrap();
 let track=legacy.create_track("Field survey").unwrap();
 legacy.track_state(track,"resume").unwrap();
 let fix=Fix{lon:106.,lat:11.,accuracy_m:Some(0.5),altitude_m:Some(20.),utc_ms:1700000000000,provider:"nmea".into(),quality:"rtk_fixed".into(),satellites:Some(18)};
 legacy.append_fix(track,&fix).unwrap();
 legacy.track_state(track,"pause").unwrap();legacy.track_state(track,"resume").unwrap();
 let second=Fix{utc_ms:fix.utc_ms+1000,lon:106.001,..fix.clone()};
 legacy.append_fix(track,&second).unwrap();
 let snap=d.path().join("snapshot.gpkg");legacy.export_snapshot(&snap).unwrap();
 let dst=d.path().join("unified.sqlite");let layer=Uuid::new_v4();
 let r=migrate(&[Source{layer,name:"survey".into(),snapshot:snap}],&dst,&AtomicBool::new(false)).unwrap();
 assert_eq!(r[0].tracks,1);assert_eq!(r[0].fixes,2);
 let s=Store::reopen(&dst).unwrap();
 let tracks=s.survey_tracks(layer,"",100,1).unwrap();
 assert_eq!(tracks[0].id,track);assert_eq!(tracks[0].state,"paused");assert_eq!(tracks[0].segment,2);
 let page=s.survey_fixes(layer,track,0,1,1).unwrap();assert_eq!(page.rows[0].fix,fix);assert_eq!(page.rows[0].segment,1);assert_eq!(page.next.as_deref(),Some("1"));
 let page=s.survey_fixes(layer,track,1,1,1).unwrap();assert_eq!(page.rows[0].fix,second);assert_eq!(page.rows[0].segment,2);
 assert!(s.integrity().unwrap());assert!(s.survey_fixes(layer,track,0,1,0).is_err());
 let old:Value=serde_json::from_str(&legacy.list_tracks().unwrap()).unwrap();assert_eq!(old[0]["state"],"recording");
}
#[test]
fn multipart_commit_keeps_holes_and_rejects_z_and_open_ring(){
 let d=tempfile::tempdir().unwrap();let root=d.path().join("projects");let p=mobile::create_project(&root).unwrap();
 let w=mobile::open_project(&root,&p).unwrap();
 let c:Value=serde_json::from_str(&w.catalog().unwrap()).unwrap();let l=c["layers"][0]["id"].as_str().unwrap();
 let f=Uuid::new_v4().to_string();let o=Uuid::new_v4().to_string();
 let g=json!({"type":"MultiPolygon","coordinates":[[[[106.,11.],[106.01,11.],[106.01,11.01],[106.,11.01],[106.,11.]],[[106.002,11.002],[106.002,11.004],[106.004,11.004],[106.004,11.002],[106.002,11.002]]],[[[107.,11.],[107.01,11.],[107.01,11.01],[107.,11.]]]]});
 w.commit_geojson(l,&f,&o,"1","0",&g.to_string(),"{}").unwrap();
 let v:Value=serde_json::from_str(&w.feature(l,&f,"2").unwrap()).unwrap();
 assert_eq!(v["geometry"]["type"],"MultiPolygon");assert_eq!(v["geometry"]["coordinates"][0].as_array().unwrap().len(),2);
 let bad=json!({"type":"Point","coordinates":[106,11,2]});
 assert_eq!(w.commit_geojson(l,&f,&Uuid::new_v4().to_string(),"2","1",&bad.to_string(),"{}"),Err(Error::Invalid));
 let bad=json!({"type":"Polygon","coordinates":[[[106,11],[107,11],[107,12],[106,12]]]});
 assert_eq!(w.commit_geojson(l,&f,&Uuid::new_v4().to_string(),"2","1",&bad.to_string(),"{}"),Err(Error::Invalid));
 assert!(w.integrity().unwrap());
}
