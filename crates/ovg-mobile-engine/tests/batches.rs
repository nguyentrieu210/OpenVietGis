use ovg_mobile_engine::{batch::{BatchHeader,WkbBatch},Error,MAX_BYTES,MAX_ROWS};
fn header()->BatchHeader{BatchHeader{schema:1,workspace:"local:1".into(),source:"source".into(),layer:"layer".into(),revision:"1".into(),epsg:4326,epoch:1,continuation:None}}
fn point()->Vec<u8>{let mut p=vec![1,1,0,0,0];p.extend(106.7_f64.to_le_bytes());p.extend(10.8_f64.to_le_bytes());p}
#[test]
fn batch_ownership_and_row_bound(){
 let p=point();let n=p.len() as u32;let b=WkbBatch::new(header(),p.clone(),vec![0,n]).unwrap();assert_eq!(b.rows(),1);assert_eq!(b.into_buffers(),(p,vec![0,n]));
 assert!(matches!(WkbBatch::new(header(),vec![0;MAX_BYTES+1],vec![0,1]),Err(Error::InvalidBatch)));
 assert!(matches!(WkbBatch::new(header(),vec![0;MAX_ROWS+1],(0..=(MAX_ROWS as u32+1)).collect()),Err(Error::InvalidBatch)));
 assert!(matches!(WkbBatch::new(header(),point(),vec![0,0,21]),Err(Error::InvalidBatch)));
}
