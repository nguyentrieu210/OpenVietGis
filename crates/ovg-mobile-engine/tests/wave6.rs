use ovg_mobile_engine::{processing::{run,Input},AlgorithmId,Error};
use geo::{Geometry,Area};
use ovg_spatial::{encode_wkb,parse_xy_wkb,ValidationLimits};
use serde_json::json;
fn row(id:&str,x:f64,y:f64)->Input{
 let g=Geometry::Polygon(geo::Rect::new(geo::Coord{x,y},geo::Coord{x:x+2.,y:y+2.}).to_polygon());
 Input{id:id.into(),wkb:encode_wkb(&g,None).unwrap(),attributes:json!({"group":"a","name":id})}
}
fn area(r:&ovg_mobile_engine::processing::ResultSet)->f64{r.rows.iter().map(|r|parse_xy_wkb(&r.wkb,&ValidationLimits::default()).unwrap().geometry.unsigned_area()).sum()}
#[test]fn independent_rectangle_areas_and_lineage(){
 let a=[row("a",0.,0.)];let b=[row("b",1.,0.)];
 for (op,want) in [(AlgorithmId::Clip,2.),(AlgorithmId::Intersection,2.),(AlgorithmId::Difference,2.),(AlgorithmId::Union,6.)]{
  let r=run(op,3857,&a,&b,&json!({}),||false,|_|{}).unwrap();assert!((area(&r)-want).abs()<1e-6);
  assert!(r.rows.iter().all(|r|!r.source_ids.is_empty()||!r.overlay_ids.is_empty()));
 }
 let r=run(AlgorithmId::Dissolve,3857,&[a[0].clone(),b[0].clone()],&[],&json!({"group_by_field":"group"}),||false,|_|{}).unwrap();
 assert!((area(&r)-6.).abs()<1e-6);assert_eq!(r.rows[0].source_ids.len(),2);
 let r=run(AlgorithmId::SpatialJoin,3857,&a,&b,&json!({}),||false,|_|{}).unwrap();
 assert_eq!(r.rows[0].attributes["overlay"]["name"],"b");
 let none=[row("c",10.,10.)];let r=run(AlgorithmId::SpatialJoin,3857,&a,&none,&json!({}),||false,|_|{}).unwrap();assert!(r.rows[0].attributes["overlay"].is_null());
}
#[test]fn buffer_projection_and_cancel(){
 let g=Geometry::Point(geo::Point::new(0.,0.));let a=[Input{id:"p".into(),wkb:encode_wkb(&g,None).unwrap(),attributes:json!({})}];
 let r=run(AlgorithmId::Buffer,3857,&a,&[],&json!({"distance_meters":10.,"quad_segs":8}),||false,|_|{}).unwrap();
 let expected=32.*0.5*100.*(std::f64::consts::TAU/32.).sin();assert!((area(&r)-expected).abs()<0.1);
 assert!(run(AlgorithmId::Buffer,4326,&a,&[],&json!({"distance_meters":10.}),||false,|_|{}).is_err());
 let p=run(AlgorithmId::Reproject,4326,&a,&[],&json!({"target_srid":3857}),||false,|_|{}).unwrap();assert_eq!(p.epsg,3857);
 assert!(matches!(run(AlgorithmId::Reproject,4326,&a,&[],&json!({"target_srid":3857}),||true,|_|{}),Err(Error::Cancelled)));
}

#[test]fn processing_job_result_once_and_workspace_close(){
 let e=ovg_mobile_engine::Engine::default();let s=e.open_session().unwrap();let w=e.open_workspace(s).unwrap();
 let j=e.start_processing(w,AlgorithmId::Clip,3857,vec![row("a",0.,0.)],vec![row("b",1.,0.)],json!({})).unwrap();
 let started=std::time::Instant::now();
 while !e.is_finished(j).unwrap(){assert!(started.elapsed()<std::time::Duration::from_secs(5));std::thread::yield_now();}
 assert_eq!(e.progress(j).unwrap(),100);assert!((area(&e.take_processing(j).unwrap())-2.).abs()<1e-6);
 assert!(matches!(e.take_processing(j),Err(Error::Stale)));
 e.close_workspace(w).unwrap();assert!(matches!(e.start_processing(w,AlgorithmId::Clip,3857,vec![],vec![],json!({})),Err(Error::Stale)));
}
