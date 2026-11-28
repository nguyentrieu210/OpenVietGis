use ovg_project_store::{Store,Error};
use sha2::{Sha256,Digest};
use std::sync::atomic::AtomicBool;
use uuid::Uuid;
#[test]
fn immutable_versions_conflicts_backup_and_restore(){
 let d=tempfile::tempdir().unwrap();let s=Store::create(&d.path().join("project.sqlite")).unwrap();
 let a=d.path().join("original.dxf");let b=d.path().join("edited.dxf");
 std::fs::write(&a,b"0\nEOF\n").unwrap();std::fs::write(&b,b"0\nSECTION\n2\nHEADER\n0\nENDSEC\n0\nEOF\n").unwrap();
 let digest=|p:&std::path::Path|format!("{:x}",Sha256::digest(std::fs::read(p).unwrap()));
 let h=digest(&a);let k=digest(&b);let p=Uuid::new_v4();let q=Uuid::new_v4();let cancel=AtomicBool::new(false);
 s.attach_cad_version(p,"cad-native-dxf",&a,0,&h,None,&cancel).unwrap();
 assert_eq!(s.attach_cad_version(q,"cad-native-dxf",&b,0,&k,Some((p,&h)),&cancel),Err(Error::Conflict));
 assert_eq!(s.attach_cad_version(q,"cad-native-dxf",&b,1,&k,Some((p,&"0".repeat(64))),&cancel),Err(Error::Conflict));
 assert_eq!(s.attach_cad_version(q,"cad-native-dxf",&b,1,&"0".repeat(64),Some((p,&h)),&cancel),Err(Error::Conflict));
 assert_eq!(s.revision().unwrap(),1);assert_eq!(s.assets().unwrap().len(),1);
 s.attach_cad_version(q,"cad-native-dxf",&b,1,&k,Some((p,&h)),&cancel).unwrap();
 let v=s.cad_versions().unwrap();assert_eq!(v[0].parent,p);assert_eq!(v[0].asset,q);
 assert_eq!(s.detach_asset(p,2),Err(Error::Relation));assert_eq!(s.revision().unwrap(),2);assert!(s.integrity().unwrap());
 let backup=d.path().join("backup.sqlite");s.backup_copy(&backup,2,&cancel).unwrap();
 let restored=d.path().join("restore.sqlite");Store::restore_copy(&backup,&restored,&cancel).unwrap();let r=Store::reopen(&restored).unwrap();
 assert_eq!(r.cad_versions().unwrap(),v);let out=d.path().join("out.dxf");r.materialize_asset(p,&out,&cancel).unwrap();assert_eq!(digest(&out),h);assert!(r.integrity().unwrap());
}
