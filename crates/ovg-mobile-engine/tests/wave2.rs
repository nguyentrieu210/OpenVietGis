use ovg_mobile_engine::{measure,viewport::{Cache,Key},staging,Error};
use std::{sync::{Arc,atomic::AtomicBool},io::Write};
use uuid::Uuid;
#[test]
fn ellipsoid_distance_bearing_and_area_oracle(){
 let s=measure::segments(4326,&[0.,0.,1.,0.]).unwrap()[0];
 assert!((s.metres-111319.49079327357).abs()<0.001);
 assert!((s.bearing_deg.unwrap()-90.).abs()<1e-9);
 let r=measure::ring(4326,&[0.,0.,1.,0.,1.,1.,0.,1.,0.,0.]).unwrap();
 assert!((r.area_m2-12308778361.469452).abs()<0.1);
 assert!((r.perimeter_m-443770.91724830196).abs()<0.001);
 assert!(measure::segments(3857,&[0.,0.,100.,100.]).is_err());
 assert!(measure::segments(4326,&[0.,91.,1.,0.]).is_err());
 assert!(measure::ring(4326,&[0.,0.,1.,0.,1.,1.,0.,1.]).is_err());
}
fn k(layer:&str,rev:u64,x:u32)->Key{Key{workspace:"local".into(),layer:layer.into(),revision:rev,style:1,zoom:2,x,y:0}}
#[test]
fn cache_epoch_scope_and_budget(){
 let mut c=Cache::new(12).unwrap();let e=c.next_epoch().unwrap();
 c.insert(e,k("a",1,0),Arc::from([1u8;8])).unwrap();
 c.insert(e,k("b",1,0),Arc::from([2u8;8])).unwrap();
 assert_eq!(c.bytes(),8);assert!(c.get(&k("a",1,0)).is_none());assert!(c.get(&k("b",2,0)).is_none());
 let n=c.next_epoch().unwrap();assert!(matches!(c.insert(e,k("a",1,1),Arc::from([1u8;4])),Err(Error::Stale)));
 c.insert(n,k("a",1,1),Arc::from([1u8;4])).unwrap();
 c.invalidate_layer("local","a");assert_eq!(c.bytes(),8);c.trim(4);assert_eq!(c.bytes(),0);
}
fn archive(path:&std::path::Path,name:&str,bytes:&[u8]){
 let f=std::fs::File::create(path).unwrap();let mut w=zip::ZipWriter::new(f);
 w.start_file(name,zip::write::SimpleFileOptions::default()).unwrap();w.write_all(bytes).unwrap();w.finish().unwrap();
}
#[test]
fn zip_traversal_cancel_and_required_companions(){
 let dir=std::env::temp_dir().join(format!("ovg-w2-{}",Uuid::new_v4()));std::fs::create_dir(&dir).unwrap();
 let zip=dir.join("archive.zip");let dst=dir.join("staged");let cancel=AtomicBool::new(false);
 archive(&zip,"../escape",b"bad");assert!(staging::unpack_zip(&zip,&dst,&cancel,1024*1024).is_err());assert!(!dst.exists());assert!(!dir.join("escape").exists());
 archive(&zip,"survey.shp",b"fixture");staging::unpack_zip(&zip,&dst,&cancel,1024*1024).unwrap();
 assert!(staging::companions(&dst.join("survey.shp")).is_err());
 for ext in ["shx","dbf","prj"]{std::fs::write(dst.join(format!("survey.{ext}")),b"fixture").unwrap();}
 assert_eq!(staging::companions(&dst.join("survey.shp")).unwrap().len(),4);
 let cancelled=AtomicBool::new(true);let stop=dir.join("stop");assert!(matches!(staging::unpack_zip(&zip,&stop,&cancelled,1024*1024),Err(Error::Cancelled)));assert!(!stop.exists());
 std::fs::remove_dir_all(dir).unwrap();
}
