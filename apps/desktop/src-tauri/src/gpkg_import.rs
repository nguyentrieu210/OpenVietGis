//! Read-only, bounded standard GeoPackage XY/EPSG:4326 single-feature-table import.
use rusqlite::{Connection,OpenFlags,types::ValueRef};
use ovg_local_storage::{LocalStore,Mutation};
use ovg_spatial::{parse_xy_wkb,encode_wkb,ValidationLimits,to_geojson,parse_geojson,PortableTransformKernel,TransformBackend,Coord2D};
use serde::Serialize;
use serde_json::{Value,Map,json};
use uuid::Uuid;
use std::path::Path;
type Result<T>=std::result::Result<T,&'static str>;
fn sql<T>(v:rusqlite::Result<T>)->Result<T>{v.map_err(|_|"INVALID_GEOPACKAGE")}
fn ident(s:&str)->Result<String>{
 if s.is_empty()||s.len()>128||s.contains('\0'){return Err("INVALID_IDENTIFIER")}
 Ok(format!("\"{}\"",s.replace('"',"\"\"")))
}
fn wkb(b:&[u8],expected:i32)->Result<Vec<u8>>{
 if b.len()<9||b.len()>4*1024*1024||&b[..2]!=b"GP"||b[2]!=0{return Err("INVALID_GEOMETRY")}
 let f=b[3];if f&0xf0!=0{return Err("UNSUPPORTED_GEOMETRY")}
 let srid=if f&1==1{i32::from_le_bytes(b[4..8].try_into().unwrap())}else{i32::from_be_bytes(b[4..8].try_into().unwrap())};
 if srid!=expected||srid<=0{return Err("UNSUPPORTED_CRS")}
 let envelope=match (f>>1)&7{0=>0,1=>32,_=>return Err("UNSUPPORTED_GEOMETRY")};
 let body=b.get(8+envelope..).ok_or("INVALID_GEOMETRY")?;
 let g=parse_xy_wkb(body,&ValidationLimits::default()).map_err(|_|"INVALID_GEOMETRY")?;
 if g.srid.is_some_and(|s|s!=expected as u32){return Err("INVALID_GEOMETRY")}
 if srid==4326{return encode_wkb(&g.geometry,None).map_err(|_|"INVALID_GEOMETRY")}
 let kernel=PortableTransformKernel::new();
 fn transform(v:&mut Value,k:&PortableTransformKernel,source:u32,depth:usize)->Result<()>{
   if depth>6{return Err("INVALID_GEOMETRY")}
   let a=v.as_array_mut().ok_or("INVALID_GEOMETRY")?;
   if a.first().is_some_and(Value::is_number){
     if a.len()!=2{return Err("UNSUPPORTED_GEOMETRY")}
     let p=k.transform_point(source,4326,Coord2D::new(a[0].as_f64().ok_or("INVALID_GEOMETRY")?,a[1].as_f64().ok_or("INVALID_GEOMETRY")?)).map_err(|_|"UNSUPPORTED_CRS")?;
     a[0]=json!(p.x);a[1]=json!(p.y);
   }else{for c in a{transform(c,k,source,depth+1)?}}
   Ok(())
 }
 let mut v=to_geojson(&g.geometry);
 transform(&mut v["coordinates"],&kernel,srid as u32,0)?;
 let g=parse_geojson(&v,&ValidationLimits::default()).map_err(|_|"INVALID_GEOMETRY")?;
 encode_wkb(&g,None).map_err(|_|"INVALID_GEOMETRY")
}
#[derive(Clone,Serialize)]
pub struct Layer {pub table:String,pub column:String,pub epsg:i32,pub z:i32,pub m:i32}
fn layers(c:&Connection)->Result<Vec<Layer>>{
 let app:i32=sql(c.pragma_query_value(None,"application_id",|r|r.get(0)))?;
 if app!=0x47504b47{return Err("INVALID_GEOPACKAGE")}
 let protected:i64=sql(c.query_row("SELECT count(*) FROM sqlite_master WHERE lower(name) LIKE '%managed%' OR lower(name) LIKE '%lease%'",[],|r|r.get(0)))?;
 if protected!=0{return Err("MANAGED_DATA_UNSUPPORTED")}
 let mut q=sql(c.prepare("SELECT g.table_name,g.column_name,g.srs_id,g.z,g.m FROM gpkg_geometry_columns g JOIN gpkg_contents c ON g.table_name=c.table_name WHERE c.data_type='features' ORDER BY g.table_name LIMIT 65"))?;
 let rows=sql(q.query_map([],|r|Ok(Layer{table:r.get(0)?,column:r.get(1)?,epsg:r.get(2)?,z:r.get(3)?,m:r.get(4)?})))?;
 let rows=sql(rows.collect::<rusqlite::Result<Vec<_>>>())?;
 if rows.is_empty()||rows.len()>64{return Err("RESOURCE_LIMIT")}
 Ok(rows)
}
pub struct Catalog {file:tempfile::NamedTempFile,pub layers:Vec<Layer>}
pub fn sqlite_snapshot(path:&Path,max:u64)->Result<tempfile::NamedTempFile>{
   let m=std::fs::symlink_metadata(path).map_err(|_|"STORAGE_UNAVAILABLE")?;
   if !m.file_type().is_file()||m.len()>max{return Err("RESOURCE_LIMIT")}
   let src=sql(Connection::open_with_flags(path,OpenFlags::SQLITE_OPEN_READ_ONLY|OpenFlags::SQLITE_OPEN_NO_MUTEX))?;
   sql(src.pragma_update(None,"trusted_schema","OFF"))?;sql(src.pragma_update(None,"query_only","ON"))?;
   sql(src.busy_timeout(std::time::Duration::from_secs(2)))?;
   let file=tempfile::NamedTempFile::new().map_err(|_|"STORAGE_UNAVAILABLE")?;
   let pages:i64=sql(src.pragma_query_value(None,"page_count",|r|r.get(0)))?;
   let size:i64=sql(src.pragma_query_value(None,"page_size",|r|r.get(0)))?;
   if pages<=0||size<=0||(pages.saturating_mul(size) as u64)>max{return Err("RESOURCE_LIMIT")}
   let mut dst=sql(Connection::open(file.path()))?;
   {let backup=sql(rusqlite::backup::Backup::new(&src,&mut dst))?;
    let until=std::time::Instant::now()+std::time::Duration::from_secs(10);
    loop{if std::time::Instant::now()>until{return Err("IMPORT_TIMEOUT")}
      match sql(backup.step(128))?{rusqlite::backup::StepResult::Done=>break,_=>std::thread::sleep(std::time::Duration::from_millis(2))}
    }
   }
   sql(dst.pragma_update(None,"trusted_schema","OFF"))?;
   let actual:i64=sql(dst.pragma_query_value(None,"page_count",|r|r.get(0)))?;
   if actual<=0||(actual.saturating_mul(size) as u64)>max{return Err("RESOURCE_LIMIT")}
   drop(dst);Ok(file)
}
impl Catalog{
 pub fn open(path:&Path)->Result<Self>{
   let file=sqlite_snapshot(path,256*1024*1024)?;
   let db=sql(Connection::open_with_flags(file.path(),OpenFlags::SQLITE_OPEN_READ_ONLY|OpenFlags::SQLITE_OPEN_NO_MUTEX))?;
   sql(db.pragma_update(None,"trusted_schema","OFF"))?;
   let layers=layers(&db)?;drop(db);Ok(Self{file,layers})
 }
 pub fn import(&self,store:&LocalStore,table:&str)->Result<u32>{import_layer(store,self.file.path(),Some(table))}
}
pub fn import(store:&LocalStore,path:&Path)->Result<u32>{import_layer(store,path,None)}
pub fn import_layer(store:&LocalStore,path:&Path,selected:Option<&str>)->Result<u32>{
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
 let available=layers(&tx)?;
 let l=match selected{Some(name)=>available.into_iter().find(|l|l.table==name).ok_or("LAYER_NOT_FOUND")?,None if available.len()==1=>available.into_iter().next().unwrap(),_=>return Err("SELECT_LAYER_REQUIRED")};
 let Layer{table,column,epsg:srid,z,m}=l;
 if srid<=0||z!=0||m!=0{return Err("UNSUPPORTED_CRS_OR_DIMENSIONS")}
 let registered:i64=sql(tx.query_row("SELECT count(*) FROM gpkg_spatial_ref_sys WHERE srs_id=?1 AND upper(organization)='EPSG' AND organization_coordsys_id=?1",[srid],|r|r.get(0)))?;
 if registered!=1{return Err("UNSUPPORTED_CRS_METADATA")}
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
   let geom=match sql(row.get_ref(gi))?{ValueRef::Blob(b)=>wkb(b,srid)?,_=>return Err("INVALID_GEOMETRY")};
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
 #[test]fn header_rejects_extended_or_wrong_srid(){let mut b=vec![b'G',b'P',0,1,0,0,0,0,1];assert!(wkb(&b,4326).is_err());b[3]=0x21;assert!(wkb(&b,4326).is_err());}
 #[test]fn quote_identifier(){assert_eq!(ident("a\"b").unwrap(),"\"a\"\"b\"");}
}
