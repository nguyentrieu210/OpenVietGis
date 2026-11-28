use ovg_managed_mobile::{Scope,Error,lease::{Claims,Clock,Lease},store::Store,sync::{Bootstrap,Feature,Commit,Receipt,Delta,Change}};
use ed25519_dalek::{SigningKey,Signer};
use zeroize::Zeroizing;
use serde_json::json;
use sha2::{Digest,Sha256};
use uuid::Uuid;
fn scope()->Scope{Scope{origin:"https://gis.example.invalid".into(),issuer:"instance-test".into(),subject:Uuid::new_v4(),organization:Uuid::new_v4(),project:Uuid::new_v4(),device:Uuid::new_v4(),session:Uuid::new_v4()}}
fn clock()->Clock{Clock{boot_id:"boot1".into(),monotonic_ms:100,wall_ms:1700000000000}}
fn lease(s:&Scope,bytes:&[u8])->Lease{
 let key=SigningKey::from_bytes(&[7;32]);let c=Claims{version:1,scope:s.clone(),key_id:"fixture".into(),package_digest:Sha256::digest(bytes).iter().map(|b|format!("{b:02x}")).collect(),permission_epoch:1,issued_ms:1000,expires_ms:60000,can_edit:true};
 let b=serde_json::to_vec(&c).unwrap();Lease::verify(&b,&key.sign(&b).to_bytes(),&key.verifying_key(),"fixture",s,2000,clock()).unwrap()
}
fn fixture()->(Scope,Uuid,Uuid,Vec<u8>){
 let s=scope();let l=Uuid::new_v4();let src=Uuid::new_v4();
 let bytes=serde_json::to_vec(&[Bootstrap{layer_id:l,source_id:src,content_revision:"a".repeat(64),schema_revision:"b".repeat(64),cursor:10,permission_epoch:1,cacheable:true,features:vec![Feature{feature_id:"feature1".into(),geometry:json!({"type":"Point","coordinates":[106.,10.]}),properties:json!({"name":"secret-fixture-xyz"}),content_revision:"a".repeat(64)}],has_more:false,next_offset:None}]).unwrap();
 (s,l,src,bytes)
}
fn temp()->std::path::PathBuf{let p=std::env::temp_dir().join(format!("ovg-managed-{}",Uuid::new_v4()));std::fs::create_dir(&p).unwrap();p}
fn open(p:&std::path::Path,s:&Scope,b:&[u8])->Store{Store::open(p,Zeroizing::new([9;32]),s.clone(),lease(s,b),&clock()).unwrap()}
#[test]
fn encrypted_snapshot_delta_receipt_and_unknown_recovery(){
 let (s,l,src,b)=fixture();let d=temp();let p=d.join("private.db");
 let mut db=open(&p,&s,&b);let c=clock();
 assert!(db.install_snapshot(&c,b"tampered").is_err());db.install_snapshot(&c,&b).unwrap();
 assert_eq!(db.cursor(&c).unwrap(),10);assert!(db.feature(&c,l,"feature1").unwrap().is_some());
 let op=Commit{session_id:s.session,source_id:src,base_content_revision:"a".repeat(64),base_schema_revision:Some("b".repeat(64)),operation_id:"mobile-op1".into(),allow_deletes:false,operations:vec![json!({"op_type":"update","feature_id":"feature1","properties":{"name":"edited"}})]};
 db.enqueue(&c,&op).unwrap();db.enqueue(&c,&op).unwrap();let sent=db.begin_send(&c,&op.operation_id).unwrap();assert_eq!(sent,serde_json::to_vec(&op).unwrap());drop(db);
 let mut db=open(&p,&s,&b);assert_eq!(db.state(&c,&op.operation_id).unwrap(),"unknown");assert!(db.begin_send(&c,&op.operation_id).is_err());
 let receipt=Receipt{status:"accepted".into(),operation_id:op.operation_id.clone(),source_id:src,resulting_content_revision:"c".repeat(64),event_cursor:14,changeset_id:Uuid::new_v4(),operation_count:1};
 db.receipt(&c,&receipt).unwrap();db.receipt(&c,&receipt).unwrap();assert_eq!(db.cursor(&c).unwrap(),10);
 let delta=Delta{reset_required:false,next_cursor:14,permission_epoch:1,has_more:false,changes:vec![Change{cursor:14,layer_id:l,source_id:src,content_revision:"c".repeat(64),upserts:vec![],removals:vec!["feature1".into()]}]};
 let data=serde_json::to_vec(&delta).unwrap();
 assert!(db.apply_delta(&c,11,&data).is_err());assert_eq!(db.cursor(&c).unwrap(),10);
 db.apply_delta(&c,10,&data).unwrap();db.apply_delta(&c,10,&data).unwrap();assert_eq!(db.cursor(&c).unwrap(),14);assert!(db.feature(&c,l,"feature1").unwrap().is_none());assert!(db.install_snapshot(&c,&b).is_err());
 for item in std::fs::read_dir(&d).unwrap(){let path=item.unwrap().path();let bytes=std::fs::read(path).unwrap();assert!(!bytes.windows(b"secret-fixture-xyz".len()).any(|w|w==b"secret-fixture-xyz"));}
 assert!(Store::open(&p,Zeroizing::new([8;32]),s.clone(),lease(&s,&b),&c).is_err());
 let mut reboot=c.clone();reboot.boot_id="boot2".into();assert!(db.cursor(&reboot).is_err());assert!(db.cursor(&c).is_err());
 db.lock().unwrap();std::fs::remove_dir_all(d).unwrap();
}
#[test]
fn lease_tamper_scope_and_expiry(){
 let (s,_,_,b)=fixture();let l=lease(&s,&b);let c=clock();
 assert!(l.check(&c,&s,1,true).is_ok());assert!(l.check(&c,&scope(),1,false).is_err());assert!(l.check(&c,&s,2,false).is_err());
 let mut expired=c.clone();expired.monotonic_ms+=60000;assert!(l.check(&expired,&s,1,false).is_err());
 let mut rollback=c.clone();rollback.wall_ms-=1;assert!(l.check(&rollback,&s,1,false).is_err());
}
#[test]
fn deterministic_100_unknown_ack_restarts(){
 let (s,_,src,b)=fixture();let d=temp();let p=d.join("private.db");let c=clock();let mut db=open(&p,&s,&b);db.install_snapshot(&c,&b).unwrap();
 for i in 0..100{
  let op=Commit{session_id:s.session,source_id:src,base_content_revision:"a".repeat(64),base_schema_revision:Some("b".repeat(64)),operation_id:format!("op-{i}"),allow_deletes:false,operations:vec![json!({"op_type":"update","feature_id":"feature1","properties":{"n":i}})]};
  db.enqueue(&c,&op).unwrap();db.begin_send(&c,&op.operation_id).unwrap();
  if i%2==0{db.unknown(&c,&op.operation_id).unwrap();}else{drop(db);db=open(&p,&s,&b);}
  assert!(matches!(db.begin_send(&c,&op.operation_id),Err(Error::Unknown)));
  let r=Receipt{status:"accepted".into(),operation_id:op.operation_id,source_id:src,resulting_content_revision:"c".repeat(64),event_cursor:11,changeset_id:Uuid::new_v4(),operation_count:1};
  db.receipt(&c,&r).unwrap();db.receipt(&c,&r).unwrap();assert_eq!(db.cursor(&c).unwrap(),10);
 }
 drop(db);std::fs::remove_dir_all(d).unwrap();
}
