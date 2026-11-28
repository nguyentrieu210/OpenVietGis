//! Read-only, bounded standard GeoPackage XY/EPSG:4326 single-feature-table import.
use rusqlite::{Connection,OpenFlags,types::ValueRef};
use ovg_local_storage::{LocalStore,Mutation};
use ovg_spatial::{parse_xy_wkb,encode_wkb,ValidationLimits};
use serde_json::{Value,Map,json};
use uuid::Uuid;
use std::path::Path;
type Result<T>=std::result::Result<T,&'static str>;
fn sql<T>(v:rusqlite::Result<T>)->Result<T>{v.map_err(|_|"INVALID_GEOPACKAGE")}
fn ident(s:&str)->Result<String>{
 if s.is_empty()||s.len()>128||s.contains('\0'){return Err("INVALID_IDENTIFIER")}
 Ok(format!("\"{}\"",s.replace('"',"\"\"")))
}
fn wkb(b:&[u8])->Result<Vec<u8>>{
 if b.len()<9||b.len()>4*1024*1024||&b[..2]!=b"GP"||b[2]!=0{return Err("INVALID_GEOMETRY")}
 let f=b[3];if f&0xf0!=0{return Err("UNSUPPORTED_GEOMETRY")}
 let srid=if f&1==1{i32::from_le_bytes(b[4..8].try_into().unwrap())}else{i32::from_be_bytes(b[4..8].try_into().unwrap())};
 if srid!=4326{return Err("UNSUPPORTED_CRS")}
 let envelope=match (f>>1)&7{0=>0,1=>32,_=>return Err("UNSUPPORTED_GEOMETRY")};
 let body=b.get(8+envelope..).ok_or("INVALID_GEOMETRY")?;
 let g=parse_xy_wkb(body,&ValidationLimits::default()).map_err(|_|"INVALID_GEOMETRY")?;
 encode_wkb(&g.geometry,None).map_err(|_|"INVALID_GEOMETRY")
}
pub fn import(store:&LocalStore,path:&Path)->Result<u32>{
 let meta=std::fs::symlink_metadata(path).map_err(|_|"STORAGE_UNAVAILABLE")?;
 if !meta.file_type().is_file()||meta.len()>256*1024*1024{return Err("RESOURCE_LIMIT")}
 let mut db=sql(Connection::open_with_flags(path,OpenFlags::SQLITE_OPEN_READ_ONLY|OpenFlags::SQLITE_OPEN_NO_MUTEX))?;
 sql(db.pragma_update(None,"trusted_schema","OFF"))?;sql(db.pragma_update(None,"query_only","ON"))?;
 sql(db.busy_timeout(std::time::Duration::from_secs(2)))?;
 let tx=sql(db.transaction())?;
 let app:i32=sql(tx.pragma_query_value(None,"application_id",|r|r.get(0)))?;
 if app!=0x47504b47{return Err("INVALID_GEOPACKAGE")}
 // Refuse known managed/private data. This is data import, never a protected-store reader.
 let protected:i64=sql(tx.query_row("SELECT count(*) FROM sqlite_master WHERE lower(name) LIKE '%managed%' OR lower(name) LIKE '%lease%'",[],|r|r.get(0)))?;
 if protected!=0{return Err("MANAGED_DATA_UNSUPPORTED")}
 let (table,column,srid,z,m)={
   let mut q=sql(tx.prepare("SELECT g.table_name,g.column_name,g.srs_id,g.z,g.m FROM gpkg_geometry_columns g JOIN gpkg_contents c ON g.table_name=c.table_name WHERE c.data_type='features' LIMIT 2"))?;
   let rows=sql(q.query_map([],|r|Ok((r.get::<_,String>(0)?,r.get::<_,String>(1)?,r.get::<_,i32>(2)?,r.get::<_,i32>(3)?,r.get::<_,i32>(4)?))))?;
   let rows=sql(rows.collect::<rusqlite::Result<Vec<_>>>())?;
   if rows.len()!=1{return Err("SINGLE_LAYER_REQUIRED")}rows.into_iter().next().unwrap()
 };
 if srid!=4326||z!=0||m!=0{return Err("UNSUPPORTED_CRS_OR_DIMENSIONS")}
 let genuine:i64=sql(tx.query_row("SELECT count(*) FROM sqlite_master WHERE name=?1 AND type='table' AND upper(ltrim(sql)) LIKE 'CREATE TABLE%'",[&table],|r|r.get(0)))?;
 if genuine!=1{return Err("UNSUPPORTED_TABLE")}
 let own:i64=sql(tx.query_row("SELECT count(*) FROM sqlite_master WHERE name='ovg_local_metadata' AND type='table'",[],|r|r.get(0)))?;
 let mut q=sql(tx.prepare(&format!("SELECT * FROM {} LIMIT 4097",ident(&table)?)))?;
 let names:Vec<String>=q.column_names().iter().map(|s|s.to_string()).collect();
 if names.len()>256{return Err("RESOURCE_LIMIT")}
 let gi=names.iter().position(|n|n==&column).ok_or("INVALID_GEOPACKAGE")?;
 let mut rows=sql(q.query([]))?;let mut mutations=Vec::new();let mut total=0usize;
 while let Some(row)=sql(rows.next())?{
   if mutations.len()>=4096{return Err("RESOURCE_LIMIT")}
   let geom=match sql(row.get_ref(gi))?{ValueRef::Blob(b)=>wkb(b)?,_=>return Err("INVALID_GEOMETRY")};
   let mut attrs=Map::new();let mut id=Uuid::new_v4();
   for (i,name) in names.iter().enumerate(){
     if i==gi{continue}
     let v=sql(row.get_ref(i))?;
     if own==1&&table=="features"{
       if name=="stable_id"{if let ValueRef::Text(b)=v{id=Uuid::parse_str(std::str::from_utf8(b).map_err(|_|"INVALID_ATTRIBUTES")?).map_err(|_|"INVALID_ATTRIBUTES")?;}else{return Err("INVALID_ATTRIBUTES")}continue}
       if name=="attributes"{if let ValueRef::Text(b)=v{if b.len()>256*1024{return Err("RESOURCE_LIMIT")}attrs=serde_json::from_slice::<Map<String,Value>>(b).map_err(|_|"INVALID_ATTRIBUTES")?;}else{return Err("INVALID_ATTRIBUTES")}continue}
       if matches!(name.as_str(),"fid"|"revision"){continue}
     }
     let val=match v{ValueRef::Null=>Value::Null,ValueRef::Integer(n)=>json!(n),ValueRef::Real(n) if n.is_finite()=>json!(n),ValueRef::Text(b) if b.len()<=256*1024=>json!(std::str::from_utf8(b).map_err(|_|"INVALID_ATTRIBUTES")?),_=>return Err("UNSUPPORTED_ATTRIBUTE")};
     attrs.insert(name.clone(),val);
   }
   let attributes=Value::Object(attrs);
   total=total.checked_add(geom.len()+attributes.to_string().len()).ok_or("RESOURCE_LIMIT")?;
   if total>16*1024*1024{return Err("RESOURCE_LIMIT")}
   mutations.push(Mutation{operation_id:Uuid::new_v4(),feature_id:id,expected_revision:0,wkb:Some(geom),attributes});
 }
 drop(rows);drop(q);sql(tx.commit())?;
 if mutations.is_empty(){return Ok(0)}
 let count=mutations.len() as u32;
 store.apply_batch(&mutations).map_err(|_|"IMPORT_TRANSACTION_FAILED")?;Ok(count)
}
#[cfg(test)]
mod tests{
 use super::*;
 #[test]fn header_rejects_extended_or_wrong_srid(){let mut b=vec![b'G',b'P',0,1,0,0,0,0,1];assert!(wkb(&b).is_err());b[3]=0x21;assert!(wkb(&b).is_err());}
 #[test]fn quote_identifier(){assert_eq!(ident("a\"b").unwrap(),"\"a\"\"b\"");}
}
