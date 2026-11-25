//! Portable GeoJSON/WKB validation and VN-2000 / WGS84 / Web Mercator CRS kernels.

pub mod geometry;
pub mod transform;

pub use geometry::{
    compute_bbox, encode_wkb, parse_geojson, parse_wkb, to_geojson, validate_geometry, BBox,
    SpatialError, SpatialGeometry, ValidationLimits, ValidationReport,
};
pub use transform::{
    Coord2D, Helmert7Params, PortableTransformKernel, TransformBackend, VerifiedReferenceProvenance,
    EPSG_6960_VN2000_TO_WGS84,
};
