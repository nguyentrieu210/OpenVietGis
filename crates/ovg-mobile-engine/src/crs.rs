//! Reuse the canonical CRS registry and portable kernel. Regional provenance is
//! never asserted by caller input. Native same-datum projection is a separate operation.
use ovg_core::crs::{reference_profile_wire,reference_transformation,CRS_REGISTRY_VERSION};
use ovg_spatial::{Coord2D,PortableTransformKernel,TransformBackend,transform::project_native_vn2000};
use crate::Error;
pub fn registry_version()->&'static str{CRS_REGISTRY_VERSION}
pub fn profile(epsg:u32)->Result<serde_json::Value,Error>{reference_profile_wire(epsg).ok_or_else(||Error::Spatial("UNSUPPORTED_CRS".into()))}
pub fn operation(source:u32,target:u32)->Result<serde_json::Value,Error>{reference_transformation(source,target).map(|x|x.to_wire()).ok_or_else(||Error::Spatial("UNSUPPORTED_TRANSFORMATION".into()))}
pub fn transform(source:u32,target:u32,x:f64,y:f64)->Result<[f64;2],Error>{
 let p=PortableTransformKernel::new().transform_point(source,target,Coord2D::new(x,y)).map_err(|e|Error::Spatial(e.to_string()))?;Ok([p.x,p.y])
}
pub fn vn2000_native(source:u32,target:u32,x:f64,y:f64)->Result<[f64;2],Error>{
 let p=project_native_vn2000(source,target,Coord2D::new(x,y)).map_err(|e|Error::Spatial(e.to_string()))?;Ok([p.x,p.y])
}
