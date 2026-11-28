use ovg_spatial::{parse_xy_wkb,parse_wkb,ValidationLimits};
fn point(raw:u32)->Vec<u8>{let mut v=vec![1];v.extend(raw.to_le_bytes());v.extend(1_f64.to_le_bytes());v.extend(2_f64.to_le_bytes());if raw==1001{v.extend(3_f64.to_le_bytes());}v}
#[test]
fn mobile_xy_never_discards_dimensions_or_embedded_crs(){
 let l=ValidationLimits::default();
 assert!(parse_xy_wkb(&point(1),&l).is_ok());
 assert!(parse_wkb(&point(1001),&l).is_ok()); // legacy behavior preserved
 assert!(parse_xy_wkb(&point(1001),&l).is_err());
 let mut collection=vec![1,7,0,0,0,1,0,0,0];collection.extend(point(1001));
 assert!(parse_xy_wkb(&collection,&l).is_err());
 let mut ewkb=vec![1];ewkb.extend(0x2000_0001_u32.to_le_bytes());ewkb.extend(4326_u32.to_le_bytes());ewkb.extend(1_f64.to_le_bytes());ewkb.extend(2_f64.to_le_bytes());
 assert!(parse_xy_wkb(&ewkb,&l).is_err());
}
