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

#[test]
fn active_survey_recovery_replay_and_backup(){
 let d=tempfile::tempdir().unwrap();let path=d.path().join("active.sqlite");
 let l=Uuid::new_v4();let t=Uuid::new_v4();let second=Uuid::new_v4();
 let f=Fix{lon:106.,lat:11.,accuracy_m:Some(4.),altitude_m:None,utc_ms:1700000000000,provider:"os".into(),quality:"unknown".into(),satellites:None};
 let s=Store::create(&path).unwrap();s.add_layer(l,"survey",0).unwrap();
 assert_eq!(s.create_survey(l,t,"Track",1).unwrap().revision,"2");
 assert_eq!(s.create_survey(l,second,"Other",2).unwrap().revision,"3");
 assert!(s.append_survey(l,t,&f,3).is_err());
 assert_eq!(s.survey_state(l,t,"resume",3).unwrap().segment,1);
 assert!(s.survey_state(l,second,"resume",4).is_err());
 let first=s.append_survey(l,t,&f,4).unwrap();assert_eq!(first.seq,Some(1));assert_eq!(first.revision,"5");
 assert_eq!(s.append_survey(l,t,&f,5).unwrap().revision,"5");
 let altered=Fix{lon:106.001,..f.clone()};assert!(s.append_survey(l,t,&altered,5).is_err());
 assert!(s.append_survey(l,t,&f,4).is_err());
 s.checkpoint().unwrap();drop(s);
 let s=Store::reopen(&path).unwrap();
 assert_eq!(s.recover_surveys(5).unwrap(),6);assert_eq!(s.recover_surveys(6).unwrap(),6);
 assert!(s.append_survey(l,t,&f,6).is_err());
 assert_eq!(s.survey_state(l,t,"resume",6).unwrap().segment,2);
 let later=Fix{utc_ms:f.utc_ms+1000,..f.clone()};
 assert_eq!(s.append_survey(l,t,&later,7).unwrap().seq,Some(2));
 assert_eq!(s.survey_state(l,t,"finish",8).unwrap().revision,"9");
 assert!(s.survey_state(l,t,"resume",9).is_err());
 let p=s.survey_fixes(l,t,0,100,9).unwrap();assert_eq!(p.rows.len(),2);assert_eq!(p.rows[0].segment,1);assert_eq!(p.rows[1].segment,2);assert_eq!(p.rows[1].fix,later);
 let b=d.path().join("backup.sqlite");s.backup_copy(&b,9,&AtomicBool::new(false)).unwrap();
 let out=d.path().join("restored.sqlite");Store::restore_copy(&b,&out,&AtomicBool::new(false)).unwrap();let r=Store::reopen(&out).unwrap();
 assert_eq!(r.survey_fixes(l,t,0,100,9).unwrap().rows.len(),2);assert!(r.integrity().unwrap());
}
