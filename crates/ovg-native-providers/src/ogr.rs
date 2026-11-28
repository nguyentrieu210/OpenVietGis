//! GDAL/OGR native vector reader and PROJ operations, restricted local profiles.
use crate::Result;
use gdal::{Dataset,DatasetOptions,GdalOpenFlags};
use gdal::spatial_ref::{SpatialRef,AxisMappingStrategy,CoordTransform,CoordTransformOptions};
use gdal::vector::{LayerAccess,FieldValue,geometry_type_has_z,geometry_type_has_m};
use serde::Serialize;
use serde_json::{Value,Map,json};
use std::{path::{Path,PathBuf},fs,io::Read};
const MAX_FILE:u64=128*1024*1024;
const MAX_BATCH:usize=64;
struct Offline;
impl Offline{
 fn new()->Result<Self>{
  // Explicit GDAL PROJ network switch. No operation enables it in this crate.
  // SAFETY: GDAL 3.4+ public C API; integer 0 disables remote grids.
  unsafe { gdal_sys::OSRSetPROJEnableNetwork(0); }
  // Scope these native library settings to this blocking worker.
  gdal::config::set_thread_local_config_option("PROJ_NETWORK","OFF").map_err(|_|"GDAL_CONFIG")?;
  gdal::config::set_thread_local_config_option("GDAL_PAM_ENABLED","NO").map_err(|_|"GDAL_CONFIG")?;
  gdal::config::set_thread_local_config_option("GDAL_DISABLE_READDIR_ON_OPEN","TRUE").map_err(|_|"GDAL_CONFIG")?;
  Ok(Self)
 }
}
impl Drop for Offline{fn drop(&mut self){for k in ["PROJ_NETWORK","GDAL_PAM_ENABLED","GDAL_DISABLE_READDIR_ON_OPEN"]{let _=gdal::config::clear_thread_local_config_option(k);}}}
fn copy(src:&Path,dst:&Path)->Result<()>{
 let m=fs::symlink_metadata(src).map_err(|_|"INVALID_FILE")?;
 if !m.file_type().is_file()||m.len()>MAX_FILE{return Err("RESOURCE_LIMIT")}
 let mut r=fs::File::open(src).map_err(|_|"INVALID_FILE")?.take(MAX_FILE+1);
 let mut w=fs::OpenOptions::new().create_new(true).write(true).open(dst).map_err(|_|"STORAGE_UNAVAILABLE")?;
 let n=std::io::copy(&mut r,&mut w).map_err(|_|"STORAGE_UNAVAILABLE")?;
 if n!=m.len(){return Err("SOURCE_CHANGED")}Ok(())
}
struct Source{_dir:tempfile::TempDir,path:PathBuf,driver:&'static str}
impl Source{
 fn vector(path:&Path)->Result<Self>{
  let ext=path.extension().and_then(|v|v.to_str()).ok_or("UNSUPPORTED_FORMAT")?.to_ascii_lowercase();
  // Deliberately no VRT, SQLite, URL-based drivers, archives or KML links.
  let (name,driver)=match ext.as_str(){"shp"=>("source.shp","ESRI Shapefile"),"fgb"=>("source.fgb","FlatGeobuf"),"parquet"=>("source.parquet","Parquet"),_=>return Err("UNSUPPORTED_FORMAT")};
  let dir=tempfile::tempdir().map_err(|_|"STORAGE_UNAVAILABLE")?;
  let out=dir.path().join(name);copy(path,&out)?;
  if ext=="shp"{
   for suffix in ["shx","dbf","prj","cpg"]{
    let p=path.with_extension(suffix);let p=if p.exists(){p}else{path.with_extension(suffix.to_ascii_uppercase())};
    if p.exists(){copy(&p,&out.with_extension(suffix))?}else if suffix!="cpg"{return Err("SHAPEFILE_SIDECAR_REQUIRED")}
   }
  }
  Ok(Self{_dir:dir,path:out,driver})
 }
 fn dataset(&self)->Result<Dataset>{
  let drivers=[self.driver];
  Dataset::open_ex(&self.path,DatasetOptions{open_flags:GdalOpenFlags::GDAL_OF_VECTOR|GdalOpenFlags::GDAL_OF_READONLY,allowed_drivers:Some(&drivers),..Default::default()}).map_err(|_|"OGR_OPEN_FAILED")
 }
}
fn epsg(s:&SpatialRef)->Result<u32>{
 if s.auth_name().as_deref()!=Some("EPSG")||s.axes_count()!=2||!(s.is_geographic()||s.is_projected()){return Err("EXPLICIT_2D_EPSG_REQUIRED")}
 let code=u32::try_from(s.auth_code().map_err(|_|"UNSUPPORTED_CRS")?).map_err(|_|"UNSUPPORTED_CRS")?;
 let wkt=s.to_wkt().map_err(|_|"UNSUPPORTED_CRS")?.to_ascii_lowercase();
 if matches!(code,3405|3406|4756)||wkt.contains("vn-2000")||wkt.contains("vietnam_2000")||wkt.contains("vietnam 2000"){return Err("USE_APPROVED_VN2000_KERNEL")}
 Ok(code)
}
fn transformer(mut source:SpatialRef,mut target:SpatialRef)->Result<CoordTransform>{
 epsg(&source)?;epsg(&target)?;
 source.set_axis_mapping_strategy(AxisMappingStrategy::TraditionalGisOrder);
 target.set_axis_mapping_strategy(AxisMappingStrategy::TraditionalGisOrder);
 let mut options=CoordTransformOptions::new().map_err(|_|"PROJ_FAILED")?;
 options.set_ballpark_allowed(false).map_err(|_|"PROJ_FAILED")?;
 CoordTransform::new_with_options(&source,&target,&options).map_err(|_|"PROJ_OPERATION_UNAVAILABLE")
}
pub fn transform(source:u32,target:u32,xy:&[[f64;2]])->Result<Vec<[f64;2]>>{
 if xy.is_empty()||xy.len()>4096||xy.iter().flatten().any(|n|!n.is_finite()){return Err("INVALID_COORDINATES")}
 let _guard=Offline::new()?;
 let tx=transformer(SpatialRef::from_epsg(source).map_err(|_|"UNSUPPORTED_CRS")?,SpatialRef::from_epsg(target).map_err(|_|"UNSUPPORTED_CRS")?)?;
 let mut x:Vec<_>=xy.iter().map(|p|p[0]).collect();let mut y:Vec<_>=xy.iter().map(|p|p[1]).collect();let mut z=vec![];
 tx.transform_coords(&mut x,&mut y,&mut z).map_err(|_|"PROJ_TRANSFORM_FAILED")?;
 if x.iter().chain(&y).any(|n|!n.is_finite()){return Err("INVALID_COORDINATES")}
 Ok(x.into_iter().zip(y).map(|(a,b)|[a,b]).collect())
}
#[derive(Serialize)]
pub struct Layer{pub index:usize,pub name:String,pub epsg:Option<u32>}
pub fn catalog(path:&Path)->Result<Vec<Layer>>{
 let _guard=Offline::new()?;let src=Source::vector(path)?;let ds=src.dataset()?;
 if ds.layer_count()>64{return Err("RESOURCE_LIMIT")}
 let mut out=Vec::new();
 for i in 0..ds.layer_count(){let l=ds.layer(i).map_err(|_|"OGR_LAYER_FAILED")?;
  out.push(Layer{index:i,name:l.name(),epsg:l.spatial_ref().and_then(|s|epsg(&s).ok())});
 }Ok(out)
}
fn field(v:Option<FieldValue>)->Result<Value>{
 Ok(match v{
  None=>Value::Null,Some(FieldValue::IntegerValue(n))=>json!(n),Some(FieldValue::Integer64Value(n))=>json!(n),
  Some(FieldValue::RealValue(n)) if n.is_finite()=>json!(n),
  Some(FieldValue::StringValue(s)) if s.len()<=256*1024=>json!(s),
  Some(FieldValue::DateValue(d))=>json!(d.to_string()),Some(FieldValue::DateTimeValue(d))=>json!(d.to_rfc3339()),
  _=>return Err("UNSUPPORTED_ATTRIBUTE")
 })
}
/// Small atomic import profile. Refuses truncation and geometry dimensional loss.
pub fn read_atomic(path:&Path,index:usize)->Result<Vec<Value>>{
 let _guard=Offline::new()?;let src=Source::vector(path)?;let ds=src.dataset()?;
 let mut l=ds.layer(index).map_err(|_|"OGR_LAYER_FAILED")?;
 let mut s=l.spatial_ref().ok_or("SOURCE_CRS_REQUIRED")?;if s.auth_name().is_none(){s.auto_identify_epsg().map_err(|_|"EXPLICIT_2D_EPSG_REQUIRED")?;}let code=epsg(&s)?;
 let tx=if code==4326{None}else{Some(transformer(s,SpatialRef::from_epsg(4326).map_err(|_|"UNSUPPORTED_CRS")?)?)};
 let mut rows=Vec::new();let mut bytes=0usize;
 for f in l.features(){
  if rows.len()>=MAX_BATCH||f.field_count()>128{return Err("ATOMIC_IMPORT_LIMIT")}
  let g=f.geometry().ok_or("INVALID_GEOMETRY")?;
  if g.is_empty()||geometry_type_has_z(g.geometry_type())||geometry_type_has_m(g.geometry_type()){return Err("UNSUPPORTED_DIMENSIONS")}
  let g=match &tx{Some(t)=>g.transform(t).map_err(|_|"PROJ_TRANSFORM_FAILED")?,None=>g.clone()};
  let geom:Value=serde_json::from_str(&g.json().map_err(|_|"INVALID_GEOMETRY")?).map_err(|_|"INVALID_GEOMETRY")?;
  let mut props=Map::new();
  for (name,value) in f.fields(){if name.len()>128||props.insert(name,field(value)?).is_some(){return Err("INVALID_ATTRIBUTE")}}
  let v=json!({"type":"Feature","geometry":geom,"properties":props});
  bytes=bytes.saturating_add(v.to_string().len());if bytes>1024*1024{return Err("RESOURCE_LIMIT")}
  rows.push(v);
 }
 if rows.is_empty(){return Err("EMPTY_LAYER")}Ok(rows)
}
#[cfg(test)]mod tests{
 use super::*;
 #[test]fn proj_axis_order_and_vn2000_gate(){let p=transform(4326,3857,&[[1.,0.]]).unwrap();assert!((p[0][0]-111319.490793).abs()<0.01);assert!(transform(3405,4326,&[[500000.,1200000.]]).is_err());}
}

#[derive(Serialize)]
pub struct RasterPreview{pub width:usize,pub height:usize,pub bands:usize,pub epsg:Option<u32>,pub geotransform:[f64;6],pub sample_width:usize,pub sample_height:usize,pub values:Vec<Option<f32>>}
/// Bounded local TIFF preview. No implicit reprojection, sidecars or full pixel transfer.
pub fn raster_preview(path:&Path,band:usize)->Result<RasterPreview>{
 let _guard=Offline::new()?;
 let ext=path.extension().and_then(|s|s.to_str()).unwrap_or("").to_ascii_lowercase();
 if !matches!(ext.as_str(),"tif"|"tiff"){return Err("UNSUPPORTED_FORMAT")}
 let dir=tempfile::tempdir().map_err(|_|"STORAGE_UNAVAILABLE")?;
 let p=dir.path().join("raster.tif");copy(path,&p)?;
 let ds=Dataset::open_ex(&p,DatasetOptions{open_flags:GdalOpenFlags::GDAL_OF_RASTER|GdalOpenFlags::GDAL_OF_READONLY,allowed_drivers:Some(&["GTiff"]),..Default::default()}).map_err(|_|"GDAL_RASTER_OPEN")?;
 let (width,height)=ds.raster_size();let bands=ds.raster_count();
 if width==0||height==0||band==0||band>bands||bands>256{return Err("INVALID_RASTER")}
 let geotransform=ds.geo_transform().map_err(|_|"RASTER_GEOREFERENCE_REQUIRED")?;
 if geotransform.iter().any(|v|!v.is_finite()){return Err("INVALID_RASTER")}
 let sample_width=width.min(128);let sample_height=height.min(128);
 let r=ds.rasterband(band).map_err(|_|"INVALID_BAND")?;
 let nodata=r.no_data_value().map(|n|n as f32);
 let data=r.read_as::<f32>((0,0),(width,height),(sample_width,sample_height),None).map_err(|_|"GDAL_RASTER_READ")?;
 let values=data.data().iter().map(|v|if v.is_finite()&&Some(*v)!=nodata{Some(*v)}else{None}).collect();
 Ok(RasterPreview{width,height,bands,epsg:ds.spatial_ref().ok().and_then(|s|epsg(&s).ok()),geotransform,sample_width,sample_height,values})
}
#[cfg(test)]mod fixture_tests{
 use super::*;
 use gdal::DriverManager;
 use gdal::vector::{Geometry,LayerOptions,OGRwkbGeometryType};
 #[test]fn shapefile_fixture_with_approved_crs(){
  let dir=tempfile::tempdir().unwrap();let path=dir.path().join("points.shp");
  let driver=DriverManager::get_driver_by_name("ESRI Shapefile").unwrap();
  let mut ds=driver.create_vector_only(&path).unwrap();
  let sr=SpatialRef::from_epsg(4326).unwrap();
  {let mut l=ds.create_layer(LayerOptions{name:"points",srs:Some(&sr),ty:OGRwkbGeometryType::wkbPoint,options:None}).unwrap();
   l.create_feature(Geometry::from_wkt("POINT (106 11)").unwrap()).unwrap();}
  drop(ds);
  let layers=catalog(&path).unwrap();assert_eq!(layers.len(),1);
  let f=read_atomic(&path,0).unwrap();assert_eq!(f.len(),1);
  assert_eq!(f[0]["geometry"]["type"],"Point");
 }
 #[test]fn refuses_vrt_and_network_paths(){
  assert!(Source::vector(Path::new("remote.vrt")).is_err());
  assert!(Source::vector(Path::new("https://example.com/remote.fgb")).is_err());
 }
}

#[derive(Clone,Debug,Serialize)]
pub struct RasterInfo{pub width:usize,pub height:usize,pub bands:usize,pub epsg:Option<u32>,pub geotransform:[f64;6]}
/// Immutable session source, reopen per operation so no GDAL handle crosses threads.
pub struct RasterSource{_dir:tempfile::TempDir,path:PathBuf,pub info:RasterInfo}
impl RasterSource{
 fn dataset(p:&Path)->Result<Dataset>{
  Dataset::open_ex(p,DatasetOptions{open_flags:GdalOpenFlags::GDAL_OF_RASTER|GdalOpenFlags::GDAL_OF_READONLY,allowed_drivers:Some(&["GTiff"]),..Default::default()}).map_err(|_|"GDAL_RASTER_OPEN")
 }
 pub fn open(source:&Path)->Result<Self>{
  let _offline=Offline::new()?;
  if !matches!(source.extension().and_then(|s|s.to_str()).unwrap_or("").to_ascii_lowercase().as_str(),"tif"|"tiff"){return Err("UNSUPPORTED_FORMAT")}
  let dir=tempfile::tempdir().map_err(|_|"STORAGE_UNAVAILABLE")?;
  let path=dir.path().join("raster.tif");
  let m=fs::symlink_metadata(source).map_err(|_|"INVALID_FILE")?;
  const LIMIT:u64=512*1024*1024;
  if !m.file_type().is_file()||m.len()>LIMIT{return Err("RESOURCE_LIMIT")}
  let mut r=fs::File::open(source).map_err(|_|"INVALID_FILE")?.take(LIMIT+1);
  let mut w=fs::OpenOptions::new().write(true).create_new(true).open(&path).map_err(|_|"STORAGE_UNAVAILABLE")?;
  let n=std::io::copy(&mut r,&mut w).map_err(|_|"STORAGE_UNAVAILABLE")?;
  if n!=m.len(){return Err("SOURCE_CHANGED")}w.sync_all().map_err(|_|"STORAGE_UNAVAILABLE")?;drop(w);
  let ds=Self::dataset(&path)?;
  let (width,height)=ds.raster_size();let bands=ds.raster_count();
  let geotransform=ds.geo_transform().map_err(|_|"RASTER_GEOREFERENCE_REQUIRED")?;
  if width==0||height==0||bands==0||bands>256||geotransform.iter().any(|v|!v.is_finite()){return Err("INVALID_RASTER")}
  let info=RasterInfo{width,height,bands,epsg:ds.spatial_ref().ok().and_then(|s|epsg(&s).ok()),geotransform};
  drop(ds);Ok(Self{_dir:dir,path,info})
 }
 /// Window in native pixel coordinates; binary little-endian Float32, NaN nodata.
 /// Hard limits on input footprint and decoded output; no remote reads.
 pub fn window(&self,band:usize,window:[usize;4],shape:[usize;2])->Result<Vec<u8>>{
  let [x,y,w,h]=window;let [ow,oh]=shape;
  if band==0||band>self.info.bands||w==0||h==0||ow==0||oh==0||ow>512||oh>512||
     w.checked_mul(h).is_none_or(|n|n>16*1024*1024)||
     x.checked_add(w).is_none_or(|n|n>self.info.width)||y.checked_add(h).is_none_or(|n|n>self.info.height){return Err("INVALID_RASTER_WINDOW")}
  let _offline=Offline::new()?;let ds=Self::dataset(&self.path)?;
  let b=ds.rasterband(band).map_err(|_|"INVALID_BAND")?;
  let nodata=b.no_data_value().map(|n|n as f32);
  let buf=b.read_as::<f32>((x as isize,y as isize),(w,h),(ow,oh),None).map_err(|_|"GDAL_RASTER_READ")?;
  let mut out=Vec::with_capacity(ow*oh*4);
  for &v in buf.data(){out.extend(if v.is_finite()&&Some(v)!=nodata{v}else{f32::NAN}.to_le_bytes());}
  Ok(out)
 }
}
/// Fixed interchange profile: UUID and attributes JSON columns plus WGS84 geometry.
/// Does not imply native typed attribute column fidelity.
pub fn export_parquet<I>(dest:&Path,rows:I)->Result<u64>
where I:IntoIterator<Item=Result<(String,Value,Value)>>{
 use gdal::{DriverManager,vector::{LayerOptions,Feature,Geometry,OGRFieldType}};
 let _offline=Offline::new()?;
 if dest.exists(){return Err("DESTINATION_EXISTS")}
 let dir=tempfile::tempdir_in(dest.parent().ok_or("INVALID_FILE")?).map_err(|_|"STORAGE_UNAVAILABLE")?;
 let path=dir.path().join("export.parquet");
 let driver=DriverManager::get_driver_by_name("Parquet").map_err(|_|"PARQUET_DRIVER_UNAVAILABLE")?;
 let mut ds=driver.create_vector_only(&path).map_err(|_|"PARQUET_CREATE_FAILED")?;
 let mut sr=SpatialRef::from_epsg(4326).map_err(|_|"UNSUPPORTED_CRS")?;
 sr.set_axis_mapping_strategy(AxisMappingStrategy::TraditionalGisOrder);
 let mut count=0u64;let mut total=0usize;
 {
  let l=ds.create_layer(LayerOptions{name:"features",srs:Some(&sr),options:Some(&["GEOMETRY_ENCODING=WKB"]),..Default::default()}).map_err(|_|"PARQUET_LAYER_FAILED")?;
  l.create_defn_fields(&[("stable_id",OGRFieldType::OFTString),("attributes_json",OGRFieldType::OFTString)]).map_err(|_|"PARQUET_SCHEMA_FAILED")?;
  for row in rows{
   let (id,g,a)=row?;let g=g.to_string();let a=a.to_string();
   total=total.saturating_add(g.len()+a.len()+id.len());
   if id.len()!=36||count>=1_000_000||total>256*1024*1024{return Err("RESOURCE_LIMIT")}
   let geom=Geometry::from_geojson(&g).map_err(|_|"INVALID_GEOMETRY")?;
   if geom.is_empty()||geometry_type_has_z(geom.geometry_type())||geometry_type_has_m(geom.geometry_type()){return Err("UNSUPPORTED_DIMENSIONS")}
   let mut f=Feature::new(l.defn()).map_err(|_|"PARQUET_WRITE_FAILED")?;
   f.set_field_string(0,&id).map_err(|_|"PARQUET_WRITE_FAILED")?;
   f.set_field_string(1,&a).map_err(|_|"PARQUET_WRITE_FAILED")?;
   f.set_geometry(geom).map_err(|_|"PARQUET_WRITE_FAILED")?;
   f.create(&l).map_err(|_|"PARQUET_WRITE_FAILED")?;count+=1;
  }
 }
 ds.flush_cache().map_err(|_|"PARQUET_WRITE_FAILED")?;drop(ds);
 let ds=Dataset::open_ex(&path,DatasetOptions{open_flags:GdalOpenFlags::GDAL_OF_VECTOR|GdalOpenFlags::GDAL_OF_READONLY,allowed_drivers:Some(&["Parquet"]),..Default::default()}).map_err(|_|"PARQUET_VERIFY_FAILED")?;
 let l=ds.layer(0).map_err(|_|"PARQUET_VERIFY_FAILED")?;
 if l.feature_count()!=count||l.spatial_ref().and_then(|s|epsg(&s).ok())!=Some(4326){return Err("PARQUET_VERIFY_FAILED")}
 drop(l);drop(ds);
 fs::File::open(&path).and_then(|f|f.sync_all()).map_err(|_|"STORAGE_UNAVAILABLE")?;
 fs::hard_link(&path,dest).map_err(|_|"DESTINATION_UNAVAILABLE")?;Ok(count)
}
