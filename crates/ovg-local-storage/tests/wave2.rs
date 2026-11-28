use ovg_local_storage::{LocalStore,Error,mbtiles::Archive};
use serde_json::{json,Value};
use uuid::Uuid;
use rusqlite::Connection;
fn tmp()->std::path::PathBuf{let p=std::env::temp_dir().join(format!("ovg-w2-{}",Uuid::new_v4()));std::fs::create_dir(&p).unwrap();p}
fn f(id:Uuid,x:f64,y:f64)->Value{json!({"type":"Feature","id":id.to_string(),"geometry":{"type":"Point","coordinates":[x,y]},"properties":{"label":"Địa điểm","null":null,"count":12}})}
fn fc(rows:Vec<Value>)->Vec<u8>{serde_json::to_vec(&json!({"type":"FeatureCollection","features":rows})).unwrap()}
#[test]
fn import_is_atomic_and_roundtrip_preserves_fields(){
 let dir=tmp();let db=LocalStore::open(&dir.join("local.gpkg"),64*1024*1024).unwrap();
 let id=Uuid::new_v4();let valid=f(id,106.7,10.8);
 let mut invalid=f(Uuid::new_v4(),106.8,10.8);invalid["geometry"]["coordinates"]=json!([106.8,10.8,25]);
 assert!(db.import_geojson(&fc(vec![valid.clone(),invalid])).is_err());
 assert!(db.query([-180.,-90.,180.,90.],0,100).unwrap().is_empty());
 assert!(db.import_geojson(&fc(vec![valid.clone(),valid.clone()])).is_err());
 assert!(db.query([-180.,-90.,180.,90.],0,100).unwrap().is_empty());
 assert_eq!(db.import_geojson(&fc(vec![valid.clone()])).unwrap(),1);
 let out:Value=serde_json::from_str(&db.export_geojson().unwrap()).unwrap();assert_eq!(out["features"][0],valid);
 let v:Value=serde_json::from_str(&db.viewport_geojson([106.,10.,107.,11.]).unwrap()).unwrap();
 assert_eq!(v["data"]["features"][0]["id"],id.to_string());assert_eq!(v["truncated"],false);
 assert_eq!(db.import_csv("x,y,label\n106.8,10.9,Điểm\n".as_bytes(),"x","y",4326,b',').unwrap(),1);
 assert!(db.import_csv(b"x,y\n1,2\ninvalid,2\n","x","y",4326,b',').is_err());
 assert_eq!(db.query([-180.,-90.,180.,90.],0,100).unwrap().len(),2);
 drop(db);std::fs::remove_dir_all(dir).unwrap();
}
#[test]
fn mbtiles_xyz_tms_and_missing_tile(){
 let dir=tmp();let path=dir.join("tiles.mbtiles");
 let db=Connection::open(&path).unwrap();
 db.execute_batch("CREATE TABLE metadata(name TEXT,value TEXT); INSERT INTO metadata VALUES ('format','png'),('attribution','fixture only'),('scheme','tms'); CREATE TABLE tiles(zoom_level INTEGER,tile_column INTEGER,tile_row INTEGER,tile_data BLOB); INSERT INTO tiles VALUES(2,1,2,X'89504E470D0A1A0A');").unwrap();
 drop(db);let a=Archive::open(&path).unwrap();assert_eq!(a.tile(2,1,1).unwrap().unwrap(),vec![137,80,78,71,13,10,26,10]);
 assert!(a.tile(2,1,2).unwrap().is_none());assert!(matches!(a.tile(2,4,0),Err(Error::Invalid)));
 drop(a);let db=Connection::open(&path).unwrap();db.execute("INSERT INTO metadata VALUES('scheme','xyz')",[]).unwrap();drop(db);assert!(Archive::open(&path).is_err());
 std::fs::remove_dir_all(dir).unwrap();
}
#[test]
fn viewport_explicitly_reports_truncation(){
 let dir=tmp();let db=LocalStore::open(&dir.join("local.gpkg"),64*1024*1024).unwrap();
 let rows=(0..513).map(|_|f(Uuid::new_v4(),106.7,10.8)).collect();assert_eq!(db.import_geojson(&fc(rows)).unwrap(),513);
 let v:Value=serde_json::from_str(&db.viewport_geojson([106.,10.,107.,11.]).unwrap()).unwrap();
 assert_eq!(v["data"]["features"].as_array().unwrap().len(),512);assert_eq!(v["truncated"],true);
 drop(db);std::fs::remove_dir_all(dir).unwrap();
}
