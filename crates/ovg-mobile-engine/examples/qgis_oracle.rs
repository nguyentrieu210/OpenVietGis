//! Actual mobile adapter measurements, using the existing isolated QGIS fixture.
use ovg_mobile_engine::{processing::{run,Input,ResultSet},AlgorithmId};
use ovg_spatial::{parse_geojson,encode_wkb,parse_xy_wkb,ValidationLimits};
use geo::Area;
use serde_json::{json,Value};
use std::path::Path;
fn read(root:&Path,name:&str)->Vec<Input>{
 let v:Value=serde_json::from_slice(&std::fs::read(root.join(name)).unwrap()).unwrap();
 assert_eq!(v["crs"]["properties"]["name"],"EPSG:3857");
 v["features"].as_array().unwrap().iter().map(|f|Input{id:f["id"].as_str().unwrap().into(),wkb:encode_wkb(&parse_geojson(&f["geometry"],&ValidationLimits::default()).unwrap(),None).unwrap(),attributes:f["properties"].clone()}).collect()
}
fn area(wkb:&[u8])->f64{parse_xy_wkb(wkb,&ValidationLimits::default()).unwrap().geometry.unsigned_area()}
fn total(r:&ResultSet)->Value{json!({"features":r.rows.len(),"area_m2":r.rows.iter().map(|r|area(&r.wkb)).sum::<f64>()})}
fn main(){
 let args=std::env::args().collect::<Vec<_>>();assert_eq!(args.len(),3);
 let root=Path::new(&args[1]);let digest=&args[2];
 let p=read(root,"parcels.geojson");let b=read(root,"clip.geojson");let point=read(root,"point.geojson");
 let clip=run(AlgorithmId::Clip,3857,&p,&b,&json!({}),||false,|_|{}).unwrap();
 let buffer=run(AlgorithmId::Buffer,3857,&point,&[],&json!({"distance_meters":10.,"quad_segs":8}),||false,|_|{}).unwrap();
 let dissolve=run(AlgorithmId::Dissolve,3857,&p,&[],&json!({"group_by_field":"zone"}),||false,|_|{}).unwrap();
 let areas=dissolve.rows.iter().map(|r|(r.attributes["zone"].as_str().unwrap().to_owned(),json!(area(&r.wkb)))).collect::<serde_json::Map<String,Value>>();
 assert!(run(AlgorithmId::Dissolve,3857,&read(root,"invalid.geojson"),&[],&json!({}),||false,|_|{}).is_err());
 println!("{}",json!({"crs":"EPSG:3857","fixture_sha256":digest,"actual":{"clip":total(&clip),"buffer":total(&buffer),"dissolve_zone":{"features":dissolve.rows.len(),"areas_m2":areas}}}));
}
