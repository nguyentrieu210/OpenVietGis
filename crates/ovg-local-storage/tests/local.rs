use ovg_local_storage::{LocalStore,Mutation,Error};
use uuid::Uuid;
use serde_json::json;
fn point(x:f64,y:f64)->Vec<u8>{let mut v=vec![1,1,0,0,0];v.extend(x.to_le_bytes());v.extend(y.to_le_bytes());v}
#[test]
fn durable_local_journal_index_replay_and_snapshot(){
    let dir=std::env::temp_dir().join(format!("ovg-local-{}",Uuid::new_v4()));
    std::fs::create_dir(&dir).unwrap();
    let path=dir.join("workspace.gpkg");
    let m=Mutation{operation_id:Uuid::new_v4(),feature_id:Uuid::new_v4(),expected_revision:0,wkb:Some(point(106.7,10.8)),attributes:json!({"name":"Địa điểm"})};
    let db=LocalStore::open(&path,32*1024*1024).unwrap();
    assert_eq!(db.apply(&m).unwrap().revision,1);
    assert!(db.apply(&m).unwrap().replayed);
    let mut modified=m.clone();modified.attributes=json!({"name":"changed"});
    assert!(matches!(db.apply(&modified),Err(Error::Reused)));
    modified.operation_id=Uuid::new_v4();
    assert!(matches!(db.apply(&modified),Err(Error::Conflict)));
    let rows=db.query([106.,10.,107.,11.],0,100).unwrap();
    assert_eq!(rows.len(),1);assert_eq!(rows[0].1.attributes,m.attributes);
    assert!(db.query([0.,0.,1.,1.],0,100).unwrap().is_empty());
    drop(db);
    let db=LocalStore::open(&path,32*1024*1024).unwrap();
    assert!(db.apply(&m).unwrap().replayed);
    let copy=dir.join("snapshot.gpkg");db.export_snapshot(&copy).unwrap();
    assert!(db.export_snapshot(&copy).is_err());
    let other=LocalStore::open(&copy,32*1024*1024).unwrap();
    assert_eq!(other.query([106.,10.,107.,11.],0,100).unwrap().len(),1);
    let delete=Mutation{operation_id:Uuid::new_v4(),feature_id:m.feature_id,expected_revision:1,wkb:None,attributes:json!({})};
    assert_eq!(db.apply(&delete).unwrap().revision,2);
    assert!(db.query([106.,10.,107.,11.],0,100).unwrap().is_empty());
    assert!(matches!(db.apply(&modified),Err(Error::Conflict)));
    db.checkpoint().unwrap();drop(db);drop(other);std::fs::remove_dir_all(dir).unwrap();
}
