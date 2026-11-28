use ovg_local_storage::{LocalStore,Mutation};
use serde_json::json;
use uuid::Uuid;
use std::{io::{BufRead,Write},process::{Command,Stdio},time::Duration};
fn mutation(id:Uuid)->Mutation{Mutation{operation_id:id,feature_id:id,expected_revision:0,wkb:Some(ovg_mobile_engine::edit::draft("point",&[106.,10.]).unwrap()),attributes:json!({"name":"acknowledged"})}}
#[test]
fn crash_child(){
 let Ok(path)=std::env::var("OVG_W7_CRASH_DB") else{return;};
 let id=Uuid::parse_str(&std::env::var("OVG_W7_CRASH_ID").unwrap()).unwrap();
 let _store=LocalStore::open(std::path::Path::new(&path),64*1024*1024).unwrap();
 assert_eq!(_store.apply(&mutation(id)).unwrap().revision,1);
 let mut conn=rusqlite::Connection::open(path).unwrap();
 let tx=conn.transaction().unwrap();
 tx.execute("UPDATE features SET attributes=?1 WHERE stable_id=?2",rusqlite::params![r#"{"name":"uncommitted"}"#,id.to_string()]).unwrap();
 println!("OVG_ACK_READY");std::io::stdout().flush().unwrap();
 loop{std::thread::sleep(Duration::from_secs(60));}
}
#[test]
fn committed_wal_survives_process_kill_and_uncommitted_write_rolls_back(){
 let dir=std::env::temp_dir().join(format!("ovg-w7-crash-{}",Uuid::new_v4()));
 std::fs::create_dir(&dir).unwrap();let db=dir.join("data.gpkg");let id=Uuid::new_v4();
 let mut child=Command::new(std::env::current_exe().unwrap()).args(["--exact","crash_child","--nocapture"]).env("OVG_W7_CRASH_DB",&db).env("OVG_W7_CRASH_ID",id.to_string()).stdout(Stdio::piped()).spawn().unwrap();
 let stdout=child.stdout.take().unwrap();
 let (send,recv)=std::sync::mpsc::channel();
 let reader=std::thread::spawn(move||{for line in std::io::BufReader::new(stdout).lines().map_while(Result::ok){if line=="OVG_ACK_READY"{let _=send.send(());break;}}});
 let ready=recv.recv_timeout(Duration::from_secs(15));
 let _=child.kill();child.wait().unwrap();reader.join().unwrap();ready.unwrap();
 let store=LocalStore::open(&db,64*1024*1024).unwrap();
 let rows=store.query([-180.,-90.,180.,90.],0,10).unwrap();
 assert_eq!(rows.len(),1);assert_eq!(rows[0].1.feature_id,id);assert_eq!(rows[0].1.revision,1);assert_eq!(rows[0].1.attributes["name"],"acknowledged");
 assert!(store.apply(&mutation(id)).unwrap().replayed);
 drop(store);std::fs::remove_dir_all(dir).unwrap();
}
#[test]
fn simulated_v2_migration_preserves_feature_and_replay(){
 let dir=std::env::temp_dir().join(format!("ovg-w7-migrate-{}",Uuid::new_v4()));std::fs::create_dir(&dir).unwrap();
 let db=dir.join("data.gpkg");let id=Uuid::new_v4();let s=LocalStore::open(&db,64*1024*1024).unwrap();
 s.apply(&mutation(id)).unwrap();s.checkpoint().unwrap();drop(s);
 let old=rusqlite::Connection::open(&db).unwrap();
 old.execute_batch("DROP TABLE ovg_edit_command; UPDATE ovg_local_metadata SET schema_version=2;").unwrap();drop(old);
 let s=LocalStore::open(&db,64*1024*1024).unwrap();assert!(s.apply(&mutation(id)).unwrap().replayed);
 assert_eq!(s.query([-180.,-90.,180.,90.],0,10).unwrap()[0].1.attributes["name"],"acknowledged");
 drop(s);std::fs::remove_dir_all(dir).unwrap();
}
