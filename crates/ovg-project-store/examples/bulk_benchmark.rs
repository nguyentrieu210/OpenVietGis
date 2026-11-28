//! Explicit opt-in reproducible bounded ingestion/viewport benchmark.
//! Run against disposable local storage only. No performance assertion.
use ovg_project_store::{Store,Image,bulk_import::BulkRow};
use std::{sync::atomic::AtomicBool,time::Instant};
use uuid::Uuid;
fn main(){
 let n=std::env::args().nth(1).map(|s|s.parse::<u64>().expect("row count")).unwrap_or(100_000);
 assert!((1..=100_000).contains(&n),"current import profile limits 100k rows");
 let d=tempfile::tempdir().expect("private fixture");
 let path=d.path().join("fixture.sqlite");
 let s=Store::create(&path).unwrap();let layer=Uuid::from_u128(1);
 let begin=Instant::now();
 let rows=(0..n).map(|i|{
  let lon=106.+(i%1000)as f64*0.00001;let lat=11.+(i/1000)as f64*0.00001;
  Ok(BulkRow{id:Uuid::from_u128(i as u128+10),image:Image{wkb:ovg_mobile_engine::edit::draft("point",&[lon,lat]).unwrap(),attributes:serde_json::json!({"sequence":i})}})
 });
 let receipt=s.import_layer_stream(Uuid::from_u128(2),layer,"benchmark",0,rows,&AtomicBool::new(false)).unwrap();
 let elapsed=begin.elapsed().as_secs_f64();s.checkpoint().unwrap();
 let file_bytes=std::fs::metadata(&path).unwrap().len();
 let mut samples=Vec::new();let mut returned=0u64;
 for i in 0..100{
  let x=106.+(i%10)as f64*0.0005;
  let t=Instant::now();
  let p=s.query(layer,[x,11.,x+0.0002,11.0002],0,200,receipt.revision).unwrap();
  samples.push(t.elapsed().as_secs_f64()*1000.);returned+=p.rows.len()as u64;
 }
 samples.sort_by(f64::total_cmp);
 assert!(s.integrity().unwrap());assert_eq!(s.report().unwrap().features,n);
 println!("{}",serde_json::json!({"fixture":"deterministic_xy_point_grid_v1","rows":n,"sqlite_bytes":file_bytes,"ingest_seconds":elapsed,"rows_per_second":n as f64/elapsed.max(1e-9),"warm_query_p50_ms":samples[49],"warm_query_p95_ms":samples[94],"query_count":samples.len(),"returned_rows":returned,"os":std::env::consts::OS,"arch":std::env::consts::ARCH,"profile":"record CPU RAM storage and build flags externally; not a qualification result"}));
}
