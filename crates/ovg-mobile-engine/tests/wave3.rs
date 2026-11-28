use ovg_mobile_engine::{edit,attributes::{self,Field,Kind},survey::{self,Fix}};
use serde_json::json;
#[test]
fn draft_topology_and_line_split_merge(){
 let a=edit::draft("line",&[106.,10.,106.01,10.01,106.02,10.01]).unwrap();
 let (l,r)=edit::split_line(&a,1).unwrap();assert_eq!(edit::merge_lines(&l,&r).unwrap(),a);
 assert!(edit::split_line(&a,0).is_err());
 assert!(edit::draft("polygon",&[0.,0.,1.,1.,0.,1.,1.,0.]).is_err());
 assert!(edit::draft("polygon",&[0.,0.,1.,0.,1.,1.,0.,1.]).is_ok());
 assert!(edit::draft("line",&[179.,0.,-179.,0.]).is_err());
 let hit=edit::snap(&edit::draft("line",&[106.,10.,106.001,10.]).unwrap(),106.0005,10.0001,30.).unwrap().unwrap();
 assert!(!hit.vertex);assert!((hit.x-106.0005).abs()<1e-8);assert!((hit.y-10.).abs()<1e-8);
}
#[test]
fn typed_schema_and_nmea(){
 let fields=[Field{name:"count".into(),kind:Kind::Integer,required:true,max_length:None,min:Some(0.),max:Some(10.)}];
 assert!(attributes::validate(&fields,&json!({"count":2})).is_ok());
 for v in [json!({"count":"2"}),json!({"count":-1}),json!({}),json!({"count":2,"unexpected":1})]{assert!(attributes::validate(&fields,&v).is_err());}
 let fix=survey::parse_gga("$GPGGA,123519,4807.038,N,01131.000,E,1,08,0.9,545.4,M,46.9,M,,*47",1700000000000).unwrap();
 assert!((fix.lat-48.1173).abs()<1e-8);assert_eq!(fix.accuracy_m,None);
 assert_eq!(fix.quality,"standalone");
 assert!(survey::parse_gga("$GPGGA,123519,4807.038,N,01131.000,E,1,08,0.9,545.4,M,46.9,M,,*00",1700000000000).is_err());
 let mut invalid=Fix{provider:"os".into(),..fix};invalid.quality="rtk_fixed".into();assert!(survey::validate_fix(&invalid).is_err());
}
