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

#[test]
fn pmtiles_local_ranges_and_hilbert(){
 use ovg_mobile_engine::pmtiles::{Archive,tile_id};
 assert_eq!([tile_id(1,0,0).unwrap(),tile_id(1,0,1).unwrap(),tile_id(1,1,1).unwrap(),tile_id(1,1,0).unwrap()],[1,2,3,4]);
 let dir=std::env::temp_dir().join(format!("ovg-pm-{}",Uuid::new_v4()));std::fs::create_dir(&dir).unwrap();let p=dir.join("a.pmtiles");
 let root=[1,0,1,3,1];let meta=br#"{"attribution":"independent fixture"}"#;let tile=[1,2,3];
 let mut h=vec![0u8;127];h[..7].copy_from_slice(b"PMTiles");h[7]=3;h[97]=1;h[98]=1;h[99]=1;
 let off=127u64+root.len()as u64;let end=off+meta.len()as u64;
 for (idx,num) in [(8,127u64),(16,root.len()as u64),(24,off),(32,meta.len()as u64),(40,end),(48,0),(56,end),(64,3)]{h[idx..idx+8].copy_from_slice(&num.to_le_bytes());}
 h.extend(root);h.extend(meta);h.extend(tile);std::fs::write(&p,&h).unwrap();
 let mut a=Archive::open(&p).unwrap();assert_eq!(a.tile(0,0,0).unwrap().unwrap(),tile);assert!(a.tile(1,0,0).unwrap().is_none());drop(a);
 h[7]=2;std::fs::write(&p,&h).unwrap();assert!(Archive::open(&p).is_err());std::fs::remove_dir_all(dir).unwrap();
}
#[test]
fn crs_registry_and_native_vn2000_oracle(){
 use ovg_mobile_engine::crs;
 assert_eq!(crs::registry_version(),"r003-v1");assert!(crs::profile(4326).is_ok());
 assert!(crs::transform(4326,999999,106.7,10.8).is_err());
 let expected=[344618.7886181846,2378231.2005045055];
 let p=crs::vn2000_native(4756,3405,103.5,21.5).unwrap();
 assert!((p[0]-expected[0]).hypot(p[1]-expected[1])<0.01);
 assert!(crs::vn2000_native(4326,3405,103.5,21.5).is_err()); // no implicit datum transform
}
