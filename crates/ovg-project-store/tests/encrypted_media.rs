use ovg_project_store::{Store,Batch,Edit,Image,Error,media::{MediaKeys,ImportRef}};
use ovg_local_storage::media::Vault;
use zeroize::Zeroizing;
use uuid::Uuid;
use std::{collections::HashMap,sync::atomic::AtomicBool};
struct Keys(HashMap<Uuid,[u8;32]>);
impl MediaKeys for Keys{
 fn key_for(&self,scope:Uuid)->Result<Zeroizing<[u8;32]>,Error>{self.0.get(&scope).copied().map(Zeroizing::new).ok_or(Error::Unsupported)}
}
#[test]
fn authenticated_media_atomic_migration_backup_and_wrong_key(){
 let d=tempfile::tempdir().unwrap();let source=Uuid::new_v4();let dest=Uuid::new_v4();
 let keys=Keys(HashMap::from([(source,[7;32]),(dest,[9;32])]));
 let v=Vault::open(&d.path().join("vault"),source,Zeroizing::new([7;32])).unwrap();
 let plain=b"private-survey-photo-fixture-2026";
 let one=v.put(plain).unwrap();let two=v.put(b"second-photo").unwrap();
 let p=d.path().join("project.sqlite");let s=Store::create(&p).unwrap();
 let l=Uuid::new_v4();let f=Uuid::new_v4();let cancel=AtomicBool::new(false);
 s.add_layer(l,"Survey",0).unwrap();
 s.apply(&Batch{operation:Uuid::new_v4(),expected_project:1,edits:vec![Edit{layer:l,feature:f,expected:0,image:Some(Image{wkb:ovg_mobile_engine::edit::draft("point",&[106.,11.]).unwrap(),attributes:serde_json::json!({})})}],links:vec![]}).unwrap();
 let a=ImportRef{layer:l,feature:f,blob:one.clone()};let mut b=ImportRef{layer:l,feature:f,blob:two.clone()};b.blob.ciphertext_sha256[0]^=1;
 let root=d.path().join("vault");
 assert!(s.import_encrypted_media(&root,source,dest,&[a.clone(),b],&keys,2,&cancel).is_err());
 assert_eq!(s.revision().unwrap(),2);assert_eq!(s.media_domain().unwrap(),None);assert!(s.media_list(l,"",100,2).unwrap().is_empty());
 let (r,items)=s.import_encrypted_media(&root,source,dest,&[a.clone()],&keys,2,&cancel).unwrap();
 assert_eq!(r,3);assert_eq!(items[0].source_id,one.id);assert_eq!(items[0].feature,f);
 let id=items[0].id;assert_eq!(&**s.read_encrypted_media(id,3,&keys).unwrap(),plain);
 assert!(s.read_encrypted_media(id,3,&Keys(HashMap::new())).is_err());
 assert!(s.read_encrypted_media(id,3,&Keys(HashMap::from([(dest,[8;32])]))).is_err());
 assert_eq!(s.import_encrypted_media(&root,source,dest,&[a],&keys,3,&cancel),Err(Error::Reused));
 assert!(s.integrity().unwrap());
 let backup=d.path().join("backup.sqlite");s.backup_copy(&backup,3,&cancel).unwrap();
 assert!(!std::fs::read(&backup).unwrap().windows(plain.len()).any(|w|w==plain));
 let dst=d.path().join("restored.sqlite");Store::restore_copy(&backup,&dst,&cancel).unwrap();
 let restored=Store::reopen(&dst).unwrap();assert_eq!(restored.media_domain().unwrap(),Some(dest));
 assert_eq!(&**restored.read_encrypted_media(id,3,&keys).unwrap(),plain);
 assert_eq!(&**v.read(&one).unwrap(),plain);
 // Changing owner metadata while keeping ciphertext hash cannot authenticate.
 let other=Uuid::new_v4();
 restored.apply(&Batch{operation:Uuid::new_v4(),expected_project:3,edits:vec![Edit{layer:l,feature:other,expected:0,image:Some(Image{wkb:ovg_mobile_engine::edit::draft("point",&[106.,11.]).unwrap(),attributes:serde_json::json!({})})}],links:vec![]}).unwrap();
 drop(restored);
 let db=rusqlite::Connection::open(&dst).unwrap();db.execute("UPDATE encrypted_media SET feature=?1",[other.to_string()]).unwrap();drop(db);
 let restored=Store::reopen(&dst).unwrap();assert!(restored.read_encrypted_media(id,4,&keys).is_err());
}

#[test]
fn native_capture_is_encrypted_and_owner_bound(){
 let d=tempfile::tempdir().unwrap();let s=Store::create(&d.path().join("p.sqlite")).unwrap();
 let scope=Uuid::new_v4();let keys=Keys(HashMap::from([(scope,[5;32])]));
 let layer=Uuid::new_v4();let feature=Uuid::new_v4();let id=Uuid::new_v4();
 s.add_layer(layer,"Survey",0).unwrap();
 s.apply(&Batch{operation:Uuid::new_v4(),expected_project:1,edits:vec![Edit{layer,feature,expected:0,image:Some(Image{wkb:ovg_mobile_engine::edit::draft("point",&[106.,11.]).unwrap(),attributes:serde_json::json!({})})}],links:vec![]}).unwrap();
 // Framing fixture only: platform decoder tests require a real device JPEG.
 let mut jpeg=vec![255,216,255];jpeg.extend(b"native capture fixture");jpeg.extend([255,217]);
 assert!(s.capture_jpeg(scope,layer,feature,id,b"invalid",2,&keys).is_err());
 assert!(s.capture_jpeg(scope,layer,Uuid::new_v4(),id,&jpeg,2,&keys).is_err());
 let(r,m)=s.capture_jpeg(scope,layer,feature,id,&jpeg,2,&keys).unwrap();assert_eq!(r,3);assert_eq!(m.id,id);assert_eq!(m.source_scope,scope);
 assert_eq!(&**s.read_encrypted_media(id,3,&keys).unwrap(),&jpeg);
 assert!(s.capture_jpeg(scope,layer,feature,id,&jpeg,3,&keys).is_err());
 assert!(s.integrity().unwrap());
 let b=d.path().join("b.sqlite");s.backup_copy(&b,3,&AtomicBool::new(false)).unwrap();
 assert!(!std::fs::read(&b).unwrap().windows(jpeg.len()).any(|w|w==jpeg.as_slice()));
}
