//! Portable GeoJSON and ISO/EWKB parser, encoder, bbox computation, and geometry validator.

use geo_types::{
    Coord, Geometry, GeometryCollection, LineString, MultiLineString, MultiPoint, MultiPolygon,
    Point, Polygon,
};
use ovg_core::AppError;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use thiserror::Error;

/// Safety limits protecting the API from giant-geometry memory exhaustion or DoS.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ValidationLimits {
    pub max_vertices: usize,
    pub max_rings_per_polygon: usize,
    pub max_nesting_depth: usize,
}

impl Default for ValidationLimits {
    fn default() -> Self {
        Self {
            max_vertices: 250_000,
            max_rings_per_polygon: 10_000,
            max_nesting_depth: 8,
        }
    }
}

/// Axis-aligned 2D bounding box `[min_x, min_y, max_x, max_y]`.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct BBox {
    pub min_x: f64,
    pub min_y: f64,
    pub max_x: f64,
    pub max_y: f64,
}

impl BBox {
    fn expand(&mut self, x: f64, y: f64) {
        if x < self.min_x {
            self.min_x = x;
        }
        if x > self.max_x {
            self.max_x = x;
        }
        if y < self.min_y {
            self.min_y = y;
        }
        if y > self.max_y {
            self.max_y = y;
        }
    }
}

/// Geometry paired with an optional explicit SRID.
#[derive(Debug, Clone, PartialEq)]
pub struct SpatialGeometry {
    pub srid: Option<u32>,
    pub geometry: Geometry<f64>,
}

/// Result of geometry validation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ValidationReport {
    pub geometry_type: &'static str,
    pub vertex_count: usize,
    pub bbox: BBox,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum SpatialError {
    #[error("invalid geometry payload: {0}")]
    InvalidPayload(String),
    #[error("geometry contains non-finite coordinate")]
    NonFiniteCoordinate,
    #[error("geometry is empty")]
    EmptyGeometry,
    #[error("polygon ring must have at least 4 coordinates and be closed")]
    UnclosedOrShortRing,
    #[error("linestring must have at least 2 coordinates")]
    ShortLineString,
    #[error("geometry exceeds vertex limit ({actual} > {limit})")]
    VertexLimitExceeded { actual: usize, limit: usize },
    #[error("polygon exceeds ring limit ({actual} > {limit})")]
    RingLimitExceeded { actual: usize, limit: usize },
    #[error("geometry nesting exceeds maximum depth ({limit})")]
    NestingDepthExceeded { limit: usize },
    #[error("invalid WKB buffer: {0}")]
    InvalidWkb(String),
}

impl From<SpatialError> for AppError {
    fn from(value: SpatialError) -> Self {
        AppError::validation("INVALID_GEOMETRY", value.to_string())
    }
}

/// Compute the 2D bounding box of any `geo_types::Geometry<f64>`.
#[must_use]
pub fn compute_bbox(geom: &Geometry<f64>) -> Option<BBox> {
    let mut acc: Option<BBox> = None;
    visit_coords(geom, &mut |c| match &mut acc {
        Some(b) => b.expand(c.x, c.y),
        None => {
            acc = Some(BBox {
                min_x: c.x,
                min_y: c.y,
                max_x: c.x,
                max_y: c.y,
            });
        }
    });
    acc
}

fn visit_coords(geom: &Geometry<f64>, f: &mut impl FnMut(Coord<f64>)) {
    match geom {
        Geometry::Point(p) => f(p.0),
        Geometry::Line(l) => {
            f(l.start);
            f(l.end);
        }
        Geometry::LineString(ls) => {
            for c in &ls.0 {
                f(*c);
            }
        }
        Geometry::Polygon(poly) => {
            for c in &poly.exterior().0 {
                f(*c);
            }
            for ring in poly.interiors() {
                for c in &ring.0 {
                    f(*c);
                }
            }
        }
        Geometry::MultiPoint(mp) => {
            for p in &mp.0 {
                f(p.0);
            }
        }
        Geometry::MultiLineString(mls) => {
            for ls in &mls.0 {
                for c in &ls.0 {
                    f(*c);
                }
            }
        }
        Geometry::MultiPolygon(mpoly) => {
            for poly in &mpoly.0 {
                for c in &poly.exterior().0 {
                    f(*c);
                }
                for ring in poly.interiors() {
                    for c in &ring.0 {
                        f(*c);
                    }
                }
            }
        }
        Geometry::GeometryCollection(gc) => {
            for inner in &gc.0 {
                visit_coords(inner, f);
            }
        }
        Geometry::Rect(r) => {
            f(r.min());
            f(r.max());
        }
        Geometry::Triangle(t) => {
            f(t.v1());
            f(t.v2());
            f(t.v3());
        }
    }
}

/// Validate coordinates, ring closure, non-emptiness, and giant-geometry limits.
pub fn validate_geometry(
    geom: &Geometry<f64>,
    limits: &ValidationLimits,
) -> Result<ValidationReport, SpatialError> {
    let mut vertex_count = 0usize;
    validate_recursive(geom, limits, 0, &mut vertex_count)?;
    let bbox = compute_bbox(geom).ok_or(SpatialError::EmptyGeometry)?;
    Ok(ValidationReport {
        geometry_type: geometry_type_name(geom),
        vertex_count,
        bbox,
    })
}

fn geometry_type_name(geom: &Geometry<f64>) -> &'static str {
    match geom {
        Geometry::Point(_) => "Point",
        Geometry::Line(_) => "Line",
        Geometry::LineString(_) => "LineString",
        Geometry::Polygon(_) => "Polygon",
        Geometry::MultiPoint(_) => "MultiPoint",
        Geometry::MultiLineString(_) => "MultiLineString",
        Geometry::MultiPolygon(_) => "MultiPolygon",
        Geometry::GeometryCollection(_) => "GeometryCollection",
        Geometry::Rect(_) => "Rect",
        Geometry::Triangle(_) => "Triangle",
    }
}

fn check_coord(
    c: Coord<f64>,
    limits: &ValidationLimits,
    vertex_count: &mut usize,
) -> Result<(), SpatialError> {
    if !c.x.is_finite() || !c.y.is_finite() {
        return Err(SpatialError::NonFiniteCoordinate);
    }
    *vertex_count += 1;
    if *vertex_count > limits.max_vertices {
        return Err(SpatialError::VertexLimitExceeded {
            actual: *vertex_count,
            limit: limits.max_vertices,
        });
    }
    Ok(())
}

fn validate_ring(
    ring: &LineString<f64>,
    limits: &ValidationLimits,
    vertex_count: &mut usize,
) -> Result<(), SpatialError> {
    let coords = &ring.0;
    if coords.len() < 4 {
        return Err(SpatialError::UnclosedOrShortRing);
    }
    for &c in coords {
        check_coord(c, limits, vertex_count)?;
    }
    let first = coords[0];
    let last = coords[coords.len() - 1];
    if first != last {
        return Err(SpatialError::UnclosedOrShortRing);
    }
    Ok(())
}

fn validate_polygon(
    poly: &Polygon<f64>,
    limits: &ValidationLimits,
    vertex_count: &mut usize,
) -> Result<(), SpatialError> {
    let total_rings = 1 + poly.interiors().len();
    if total_rings > limits.max_rings_per_polygon {
        return Err(SpatialError::RingLimitExceeded {
            actual: total_rings,
            limit: limits.max_rings_per_polygon,
        });
    }
    validate_ring(poly.exterior(), limits, vertex_count)?;
    for ring in poly.interiors() {
        validate_ring(ring, limits, vertex_count)?;
    }
    Ok(())
}

fn validate_recursive(
    geom: &Geometry<f64>,
    limits: &ValidationLimits,
    depth: usize,
    vertex_count: &mut usize,
) -> Result<(), SpatialError> {
    if depth > limits.max_nesting_depth {
        return Err(SpatialError::NestingDepthExceeded {
            limit: limits.max_nesting_depth,
        });
    }
    match geom {
        Geometry::Point(p) => check_coord(p.0, limits, vertex_count),
        Geometry::Line(l) => {
            check_coord(l.start, limits, vertex_count)?;
            check_coord(l.end, limits, vertex_count)
        }
        Geometry::LineString(ls) => {
            if ls.0.len() < 2 {
                return Err(SpatialError::ShortLineString);
            }
            for &c in &ls.0 {
                check_coord(c, limits, vertex_count)?;
            }
            Ok(())
        }
        Geometry::Polygon(poly) => validate_polygon(poly, limits, vertex_count),
        Geometry::MultiPoint(mp) => {
            if mp.0.is_empty() {
                return Err(SpatialError::EmptyGeometry);
            }
            for p in &mp.0 {
                check_coord(p.0, limits, vertex_count)?;
            }
            Ok(())
        }
        Geometry::MultiLineString(mls) => {
            if mls.0.is_empty() {
                return Err(SpatialError::EmptyGeometry);
            }
            for ls in &mls.0 {
                if ls.0.len() < 2 {
                    return Err(SpatialError::ShortLineString);
                }
                for &c in &ls.0 {
                    check_coord(c, limits, vertex_count)?;
                }
            }
            Ok(())
        }
        Geometry::MultiPolygon(mpoly) => {
            if mpoly.0.is_empty() {
                return Err(SpatialError::EmptyGeometry);
            }
            for poly in &mpoly.0 {
                validate_polygon(poly, limits, vertex_count)?;
            }
            Ok(())
        }
        Geometry::GeometryCollection(gc) => {
            if gc.0.is_empty() {
                return Err(SpatialError::EmptyGeometry);
            }
            for inner in &gc.0 {
                validate_recursive(inner, limits, depth + 1, vertex_count)?;
            }
            Ok(())
        }
        Geometry::Rect(r) => {
            check_coord(r.min(), limits, vertex_count)?;
            check_coord(r.max(), limits, vertex_count)
        }
        Geometry::Triangle(t) => {
            check_coord(t.v1(), limits, vertex_count)?;
            check_coord(t.v2(), limits, vertex_count)?;
            check_coord(t.v3(), limits, vertex_count)
        }
    }
}

/// Parse a GeoJSON Geometry object into a `geo_types::Geometry<f64>` with strict validation.
pub fn parse_geojson(
    value: &Value,
    limits: &ValidationLimits,
) -> Result<Geometry<f64>, SpatialError> {
    let mut vertex_budget = 0usize;
    parse_geojson_recursive(value, limits, 0, &mut vertex_budget)
}

fn parse_coord(
    v: &Value,
    limits: &ValidationLimits,
    count: &mut usize,
) -> Result<Coord<f64>, SpatialError> {
    let arr = v
        .as_array()
        .ok_or_else(|| SpatialError::InvalidPayload("coordinate must be an array".to_string()))?;
    if arr.len() < 2 {
        return Err(SpatialError::InvalidPayload(
            "coordinate must have at least [x, y]".to_string(),
        ));
    }
    let x = arr[0]
        .as_f64()
        .ok_or_else(|| SpatialError::InvalidPayload("x coordinate must be numeric".to_string()))?;
    let y = arr[1]
        .as_f64()
        .ok_or_else(|| SpatialError::InvalidPayload("y coordinate must be numeric".to_string()))?;
    let coord = Coord { x, y };
    check_coord(coord, limits, count)?;
    Ok(coord)
}

fn parse_coord_seq(
    v: &Value,
    limits: &ValidationLimits,
    count: &mut usize,
) -> Result<Vec<Coord<f64>>, SpatialError> {
    let arr = v.as_array().ok_or_else(|| {
        SpatialError::InvalidPayload("coordinate list must be an array".to_string())
    })?;
    let mut out = Vec::with_capacity(arr.len());
    for item in arr {
        out.push(parse_coord(item, limits, count)?);
    }
    Ok(out)
}

fn parse_polygon_coords(
    v: &Value,
    limits: &ValidationLimits,
    count: &mut usize,
) -> Result<Polygon<f64>, SpatialError> {
    let rings_val = v.as_array().ok_or_else(|| {
        SpatialError::InvalidPayload("polygon rings must be an array".to_string())
    })?;
    if rings_val.is_empty() {
        return Err(SpatialError::EmptyGeometry);
    }
    if rings_val.len() > limits.max_rings_per_polygon {
        return Err(SpatialError::RingLimitExceeded {
            actual: rings_val.len(),
            limit: limits.max_rings_per_polygon,
        });
    }
    let mut rings = Vec::with_capacity(rings_val.len());
    for ring_val in rings_val {
        let coords = parse_coord_seq(ring_val, limits, count)?;
        if coords.len() < 4 || coords[0] != coords[coords.len() - 1] {
            return Err(SpatialError::UnclosedOrShortRing);
        }
        rings.push(LineString(coords));
    }
    let exterior = rings.remove(0);
    Ok(Polygon::new(exterior, rings))
}

fn parse_geojson_recursive(
    value: &Value,
    limits: &ValidationLimits,
    depth: usize,
    count: &mut usize,
) -> Result<Geometry<f64>, SpatialError> {
    if depth > limits.max_nesting_depth {
        return Err(SpatialError::NestingDepthExceeded {
            limit: limits.max_nesting_depth,
        });
    }
    let obj = value.as_object().ok_or_else(|| {
        SpatialError::InvalidPayload("geometry must be a JSON object".to_string())
    })?;
    let gtype = obj
        .get("type")
        .and_then(Value::as_str)
        .ok_or_else(|| SpatialError::InvalidPayload("missing geometry type".to_string()))?;

    match gtype {
        "Point" => {
            let coords = obj
                .get("coordinates")
                .ok_or_else(|| SpatialError::InvalidPayload("missing coordinates".to_string()))?;
            Ok(Geometry::Point(Point(parse_coord(coords, limits, count)?)))
        }
        "LineString" => {
            let coords = obj
                .get("coordinates")
                .ok_or_else(|| SpatialError::InvalidPayload("missing coordinates".to_string()))?;
            let pts = parse_coord_seq(coords, limits, count)?;
            if pts.len() < 2 {
                return Err(SpatialError::ShortLineString);
            }
            Ok(Geometry::LineString(LineString(pts)))
        }
        "Polygon" => {
            let coords = obj
                .get("coordinates")
                .ok_or_else(|| SpatialError::InvalidPayload("missing coordinates".to_string()))?;
            Ok(Geometry::Polygon(parse_polygon_coords(
                coords, limits, count,
            )?))
        }
        "MultiPoint" => {
            let coords = obj
                .get("coordinates")
                .ok_or_else(|| SpatialError::InvalidPayload("missing coordinates".to_string()))?;
            let pts = parse_coord_seq(coords, limits, count)?;
            if pts.is_empty() {
                return Err(SpatialError::EmptyGeometry);
            }
            Ok(Geometry::MultiPoint(MultiPoint(
                pts.into_iter().map(Point).collect(),
            )))
        }
        "MultiLineString" => {
            let arr = obj
                .get("coordinates")
                .and_then(Value::as_array)
                .ok_or_else(|| SpatialError::InvalidPayload("missing coordinates".to_string()))?;
            if arr.is_empty() {
                return Err(SpatialError::EmptyGeometry);
            }
            let mut lines = Vec::with_capacity(arr.len());
            for item in arr {
                let pts = parse_coord_seq(item, limits, count)?;
                if pts.len() < 2 {
                    return Err(SpatialError::ShortLineString);
                }
                lines.push(LineString(pts));
            }
            Ok(Geometry::MultiLineString(MultiLineString(lines)))
        }
        "MultiPolygon" => {
            let arr = obj
                .get("coordinates")
                .and_then(Value::as_array)
                .ok_or_else(|| SpatialError::InvalidPayload("missing coordinates".to_string()))?;
            if arr.is_empty() {
                return Err(SpatialError::EmptyGeometry);
            }
            let mut polys = Vec::with_capacity(arr.len());
            for item in arr {
                polys.push(parse_polygon_coords(item, limits, count)?);
            }
            Ok(Geometry::MultiPolygon(MultiPolygon(polys)))
        }
        "GeometryCollection" => {
            let arr = obj
                .get("geometries")
                .and_then(Value::as_array)
                .ok_or_else(|| SpatialError::InvalidPayload("missing geometries".to_string()))?;
            if arr.is_empty() {
                return Err(SpatialError::EmptyGeometry);
            }
            let mut geoms = Vec::with_capacity(arr.len());
            for item in arr {
                geoms.push(parse_geojson_recursive(item, limits, depth + 1, count)?);
            }
            Ok(Geometry::GeometryCollection(GeometryCollection(geoms)))
        }
        other => Err(SpatialError::InvalidPayload(format!(
            "unsupported GeoJSON geometry type: {other}"
        ))),
    }
}

/// Serialize a `geo_types::Geometry<f64>` into a GeoJSON `serde_json::Value`.
#[must_use]
pub fn to_geojson(geom: &Geometry<f64>) -> Value {
    let coord_json = |c: &Coord<f64>| json!([c.x, c.y]);
    let ls_json = |ls: &LineString<f64>| Value::Array(ls.0.iter().map(coord_json).collect());
    let poly_json = |poly: &Polygon<f64>| {
        let mut rings = Vec::with_capacity(1 + poly.interiors().len());
        rings.push(ls_json(poly.exterior()));
        for ring in poly.interiors() {
            rings.push(ls_json(ring));
        }
        Value::Array(rings)
    };

    match geom {
        Geometry::Point(p) => json!({ "type": "Point", "coordinates": coord_json(&p.0) }),
        Geometry::Line(l) => json!({
            "type": "LineString",
            "coordinates": [coord_json(&l.start), coord_json(&l.end)]
        }),
        Geometry::LineString(ls) => json!({ "type": "LineString", "coordinates": ls_json(ls) }),
        Geometry::Polygon(poly) => json!({ "type": "Polygon", "coordinates": poly_json(poly) }),
        Geometry::MultiPoint(mp) => json!({
            "type": "MultiPoint",
            "coordinates": mp.0.iter().map(|p| coord_json(&p.0)).collect::<Vec<_>>()
        }),
        Geometry::MultiLineString(mls) => json!({
            "type": "MultiLineString",
            "coordinates": mls.0.iter().map(ls_json).collect::<Vec<_>>()
        }),
        Geometry::MultiPolygon(mpoly) => json!({
            "type": "MultiPolygon",
            "coordinates": mpoly.0.iter().map(poly_json).collect::<Vec<_>>()
        }),
        Geometry::GeometryCollection(gc) => json!({
            "type": "GeometryCollection",
            "geometries": gc.0.iter().map(to_geojson).collect::<Vec<_>>()
        }),
        Geometry::Rect(r) => to_geojson(&Geometry::Polygon(r.to_polygon())),
        Geometry::Triangle(t) => to_geojson(&Geometry::Polygon(t.to_polygon())),
    }
}

const WKB_SRID_FLAG: u32 = 0x2000_0000;
const WKB_Z_FLAG: u32 = 0x8000_0000;
const WKB_M_FLAG: u32 = 0x4000_0000;

struct WkbReader<'a> {
    bytes: &'a [u8],
    offset: usize,
}

impl<'a> WkbReader<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, offset: 0 }
    }

    fn read_u8(&mut self) -> Result<u8, SpatialError> {
        if self.offset >= self.bytes.len() {
            return Err(SpatialError::InvalidWkb("unexpected EOF".to_string()));
        }
        let b = self.bytes[self.offset];
        self.offset += 1;
        Ok(b)
    }

    fn read_u32(&mut self, little_endian: bool) -> Result<u32, SpatialError> {
        if self.offset + 4 > self.bytes.len() {
            return Err(SpatialError::InvalidWkb(
                "unexpected EOF reading u32".to_string(),
            ));
        }
        let slice: [u8; 4] = self.bytes[self.offset..self.offset + 4]
            .try_into()
            .map_err(|_| SpatialError::InvalidWkb("slice error".to_string()))?;
        self.offset += 4;
        Ok(if little_endian {
            u32::from_le_bytes(slice)
        } else {
            u32::from_be_bytes(slice)
        })
    }

    fn read_f64(&mut self, little_endian: bool) -> Result<f64, SpatialError> {
        if self.offset + 8 > self.bytes.len() {
            return Err(SpatialError::InvalidWkb(
                "unexpected EOF reading f64".to_string(),
            ));
        }
        let slice: [u8; 8] = self.bytes[self.offset..self.offset + 8]
            .try_into()
            .map_err(|_| SpatialError::InvalidWkb("slice error".to_string()))?;
        self.offset += 8;
        Ok(if little_endian {
            f64::from_le_bytes(slice)
        } else {
            f64::from_be_bytes(slice)
        })
    }

    fn read_coord(
        &mut self,
        little_endian: bool,
        has_z: bool,
        has_m: bool,
        limits: &ValidationLimits,
        vertex_count: &mut usize,
    ) -> Result<Coord<f64>, SpatialError> {
        let x = self.read_f64(little_endian)?;
        let y = self.read_f64(little_endian)?;
        if has_z {
            let z = self.read_f64(little_endian)?;
            if !z.is_finite() {
                return Err(SpatialError::NonFiniteCoordinate);
            }
        }
        if has_m {
            let m = self.read_f64(little_endian)?;
            if !m.is_finite() {
                return Err(SpatialError::NonFiniteCoordinate);
            }
        }
        let coord = Coord { x, y };
        check_coord(coord, limits, vertex_count)?;
        Ok(coord)
    }

    fn read_geometry(
        &mut self,
        limits: &ValidationLimits,
        depth: usize,
        vertex_count: &mut usize,
    ) -> Result<SpatialGeometry, SpatialError> {
        if depth > limits.max_nesting_depth {
            return Err(SpatialError::NestingDepthExceeded {
                limit: limits.max_nesting_depth,
            });
        }
        let order = self.read_u8()?;
        let le = match order {
            1 => true,
            0 => false,
            _ => {
                return Err(SpatialError::InvalidWkb(format!(
                    "invalid byte order {order}"
                )))
            }
        };
        let raw_type = self.read_u32(le)?;
        let has_srid = (raw_type & WKB_SRID_FLAG) != 0;
        let has_z = (raw_type & WKB_Z_FLAG) != 0
            || ((raw_type & 0xFFFF) / 1000 == 1)
            || ((raw_type & 0xFFFF) / 1000 == 3);
        let has_m = (raw_type & WKB_M_FLAG) != 0
            || ((raw_type & 0xFFFF) / 1000 == 2)
            || ((raw_type & 0xFFFF) / 1000 == 3);
        let base_type = (raw_type & 0x0FFF_FFFF) % 1000;

        let srid = if has_srid {
            Some(self.read_u32(le)?)
        } else {
            None
        };

        let geometry = match base_type {
            1 => {
                let c = self.read_coord(le, has_z, has_m, limits, vertex_count)?;
                Geometry::Point(Point(c))
            }
            2 => {
                let n = self.read_u32(le)? as usize;
                if n < 2 {
                    return Err(SpatialError::ShortLineString);
                }
                let mut coords = Vec::with_capacity(n.min(limits.max_vertices));
                for _ in 0..n {
                    coords.push(self.read_coord(le, has_z, has_m, limits, vertex_count)?);
                }
                Geometry::LineString(LineString(coords))
            }
            3 => {
                let num_rings = self.read_u32(le)? as usize;
                if num_rings == 0 {
                    return Err(SpatialError::EmptyGeometry);
                }
                if num_rings > limits.max_rings_per_polygon {
                    return Err(SpatialError::RingLimitExceeded {
                        actual: num_rings,
                        limit: limits.max_rings_per_polygon,
                    });
                }
                let mut rings = Vec::with_capacity(num_rings);
                for _ in 0..num_rings {
                    let n = self.read_u32(le)? as usize;
                    if n < 4 {
                        return Err(SpatialError::UnclosedOrShortRing);
                    }
                    let mut coords = Vec::with_capacity(n.min(limits.max_vertices));
                    for _ in 0..n {
                        coords.push(self.read_coord(le, has_z, has_m, limits, vertex_count)?);
                    }
                    if coords[0] != coords[coords.len() - 1] {
                        return Err(SpatialError::UnclosedOrShortRing);
                    }
                    rings.push(LineString(coords));
                }
                let exterior = rings.remove(0);
                Geometry::Polygon(Polygon::new(exterior, rings))
            }
            4 => {
                let n = self.read_u32(le)? as usize;
                if n == 0 {
                    return Err(SpatialError::EmptyGeometry);
                }
                let mut pts = Vec::with_capacity(n.min(limits.max_vertices));
                for _ in 0..n {
                    match self
                        .read_geometry(limits, depth + 1, vertex_count)?
                        .geometry
                    {
                        Geometry::Point(p) => pts.push(p),
                        _ => {
                            return Err(SpatialError::InvalidWkb(
                                "expected Point in MultiPoint".to_string(),
                            ))
                        }
                    }
                }
                Geometry::MultiPoint(MultiPoint(pts))
            }
            5 => {
                let n = self.read_u32(le)? as usize;
                if n == 0 {
                    return Err(SpatialError::EmptyGeometry);
                }
                let mut lines = Vec::with_capacity(n.min(limits.max_vertices));
                for _ in 0..n {
                    match self
                        .read_geometry(limits, depth + 1, vertex_count)?
                        .geometry
                    {
                        Geometry::LineString(ls) => lines.push(ls),
                        _ => {
                            return Err(SpatialError::InvalidWkb(
                                "expected LineString in MultiLineString".to_string(),
                            ))
                        }
                    }
                }
                Geometry::MultiLineString(MultiLineString(lines))
            }
            6 => {
                let n = self.read_u32(le)? as usize;
                if n == 0 {
                    return Err(SpatialError::EmptyGeometry);
                }
                let mut polys = Vec::with_capacity(n.min(limits.max_vertices));
                for _ in 0..n {
                    match self
                        .read_geometry(limits, depth + 1, vertex_count)?
                        .geometry
                    {
                        Geometry::Polygon(p) => polys.push(p),
                        _ => {
                            return Err(SpatialError::InvalidWkb(
                                "expected Polygon in MultiPolygon".to_string(),
                            ))
                        }
                    }
                }
                Geometry::MultiPolygon(MultiPolygon(polys))
            }
            7 => {
                let n = self.read_u32(le)? as usize;
                if n == 0 {
                    return Err(SpatialError::EmptyGeometry);
                }
                let mut geoms = Vec::with_capacity(n.min(limits.max_vertices));
                for _ in 0..n {
                    geoms.push(
                        self.read_geometry(limits, depth + 1, vertex_count)?
                            .geometry,
                    );
                }
                Geometry::GeometryCollection(GeometryCollection(geoms))
            }
            other => {
                return Err(SpatialError::InvalidWkb(format!(
                    "unsupported WKB geometry type {other}"
                )));
            }
        };

        Ok(SpatialGeometry { srid, geometry })
    }
}

/// Parse ISO WKB or PostGIS EWKB bytes with strict vertex/ring safety limits.
pub fn parse_wkb(bytes: &[u8], limits: &ValidationLimits) -> Result<SpatialGeometry, SpatialError> {
    let mut reader = WkbReader::new(bytes);
    let mut vertex_count = 0usize;
    let parsed = reader.read_geometry(limits, 0, &mut vertex_count)?;
    if reader.offset != bytes.len() {
        return Err(SpatialError::InvalidWkb(
            "trailing bytes after WKB geometry".to_string(),
        ));
    }
    Ok(parsed)
}

/// Encode a 2D `geo_types::Geometry<f64>` as Little-Endian ISO WKB or PostGIS EWKB (when `srid` is `Some`).
pub fn encode_wkb(geom: &Geometry<f64>, srid: Option<u32>) -> Result<Vec<u8>, SpatialError> {
    let mut buf = Vec::new();
    write_wkb_inner(geom, srid, &mut buf)?;
    Ok(buf)
}

fn write_wkb_header(buf: &mut Vec<u8>, base_type: u32, srid: Option<u32>) {
    buf.push(1u8); // NDR (Little Endian)
    match srid {
        Some(code) => {
            buf.extend_from_slice(&(base_type | WKB_SRID_FLAG).to_le_bytes());
            buf.extend_from_slice(&code.to_le_bytes());
        }
        None => {
            buf.extend_from_slice(&base_type.to_le_bytes());
        }
    }
}

fn write_coord(buf: &mut Vec<u8>, c: Coord<f64>) -> Result<(), SpatialError> {
    if !c.x.is_finite() || !c.y.is_finite() {
        return Err(SpatialError::NonFiniteCoordinate);
    }
    buf.extend_from_slice(&c.x.to_le_bytes());
    buf.extend_from_slice(&c.y.to_le_bytes());
    Ok(())
}

// Borrow nested geometry directly. Encoding does not modify the source and
// copying every coordinate of each member is unnecessary.
fn write_wkb_linestring(
    ls: &LineString<f64>,
    srid: Option<u32>,
    buf: &mut Vec<u8>,
) -> Result<(), SpatialError> {
    write_wkb_header(buf, 2, srid);
    buf.extend_from_slice(&(ls.0.len() as u32).to_le_bytes());
    for &c in &ls.0 {
        write_coord(buf, c)?;
    }
    Ok(())
}

fn write_wkb_polygon(
    poly: &Polygon<f64>,
    srid: Option<u32>,
    buf: &mut Vec<u8>,
) -> Result<(), SpatialError> {
    write_wkb_header(buf, 3, srid);
    let ring_count = (1 + poly.interiors().len()) as u32;
    buf.extend_from_slice(&ring_count.to_le_bytes());
    for ring in std::iter::once(poly.exterior()).chain(poly.interiors()) {
        buf.extend_from_slice(&(ring.0.len() as u32).to_le_bytes());
        for &c in &ring.0 {
            write_coord(buf, c)?;
        }
    }
    Ok(())
}

fn write_wkb_inner(
    geom: &Geometry<f64>,
    srid: Option<u32>,
    buf: &mut Vec<u8>,
) -> Result<(), SpatialError> {
    match geom {
        Geometry::Point(p) => {
            write_wkb_header(buf, 1, srid);
            write_coord(buf, p.0)?;
        }
        Geometry::Line(l) => {
            write_wkb_header(buf, 2, srid);
            buf.extend_from_slice(&2u32.to_le_bytes());
            write_coord(buf, l.start)?;
            write_coord(buf, l.end)?;
        }
        Geometry::LineString(ls) => write_wkb_linestring(ls, srid, buf)?,
        Geometry::Polygon(poly) => write_wkb_polygon(poly, srid, buf)?,
        Geometry::MultiPoint(mp) => {
            write_wkb_header(buf, 4, srid);
            buf.extend_from_slice(&(mp.0.len() as u32).to_le_bytes());
            for p in &mp.0 {
                write_wkb_inner(&Geometry::Point(*p), None, buf)?;
            }
        }
        Geometry::MultiLineString(mls) => {
            write_wkb_header(buf, 5, srid);
            buf.extend_from_slice(&(mls.0.len() as u32).to_le_bytes());
            for ls in &mls.0 {
                write_wkb_linestring(ls, None, buf)?;
            }
        }
        Geometry::MultiPolygon(mpoly) => {
            write_wkb_header(buf, 6, srid);
            buf.extend_from_slice(&(mpoly.0.len() as u32).to_le_bytes());
            for poly in &mpoly.0 {
                write_wkb_polygon(poly, None, buf)?;
            }
        }
        Geometry::GeometryCollection(gc) => {
            write_wkb_header(buf, 7, srid);
            buf.extend_from_slice(&(gc.0.len() as u32).to_le_bytes());
            for inner in &gc.0 {
                write_wkb_inner(inner, None, buf)?;
            }
        }
        Geometry::Rect(r) => write_wkb_inner(&Geometry::Polygon(r.to_polygon()), srid, buf)?,
        Geometry::Triangle(t) => write_wkb_inner(&Geometry::Polygon(t.to_polygon()), srid, buf)?,
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn parses_and_validates_geojson_and_ewkb_roundtrip() {
        let poly_json = json!({
            "type": "Polygon",
            "coordinates": [[[105.8, 21.0], [105.9, 21.0], [105.9, 21.1], [105.8, 21.0]]]
        });
        let limits = ValidationLimits::default();
        let geom = parse_geojson(&poly_json, &limits).unwrap();
        let report = validate_geometry(&geom, &limits).unwrap();
        assert_eq!(report.geometry_type, "Polygon");
        assert_eq!(report.vertex_count, 4);
        assert_eq!(report.bbox.min_x, 105.8);
        assert_eq!(report.bbox.max_y, 21.1);

        let ewkb = encode_wkb(&geom, Some(4326)).unwrap();
        let decoded = parse_wkb(&ewkb, &limits).unwrap();
        assert_eq!(decoded.srid, Some(4326));
        assert_eq!(decoded.geometry, geom);
        assert_eq!(to_geojson(&decoded.geometry), poly_json);
    }

    #[test]
    fn multipart_encoding_preserves_child_bytes_and_parent_srid() {
        let limits = ValidationLimits::default();
        for input in [
            json!({"type":"MultiLineString","coordinates":[[[1.0,2.0],[3.0,4.0]],[[5.0,6.0],[7.0,8.0]]]}),
            json!({"type":"MultiPolygon","coordinates":[[[[0.0,0.0],[3.0,0.0],[3.0,3.0],[0.0,0.0]],[[1.0,1.0],[2.0,1.0],[2.0,2.0],[1.0,1.0]]],[[[4.0,4.0],[5.0,4.0],[5.0,5.0],[4.0,4.0]]]]}),
        ] {
            let geom = parse_geojson(&input, &limits).unwrap();
            let (kind, children): (u32, Vec<Geometry<f64>>) = match &geom {
                Geometry::MultiLineString(lines) => (
                    5,
                    lines.0.iter().cloned().map(Geometry::LineString).collect(),
                ),
                Geometry::MultiPolygon(polygons) => (
                    6,
                    polygons.0.iter().cloned().map(Geometry::Polygon).collect(),
                ),
                _ => unreachable!(),
            };
            for srid in [None, Some(4326)] {
                let mut expected = Vec::new();
                write_wkb_header(&mut expected, kind, srid);
                expected.extend_from_slice(&(children.len() as u32).to_le_bytes());
                for child in &children {
                    expected.extend(encode_wkb(child, None).unwrap());
                }
                let encoded = encode_wkb(&geom, srid).unwrap();
                assert_eq!(encoded, expected);
                let decoded = parse_wkb(&encoded, &limits).unwrap();
                assert_eq!(decoded.geometry, geom);
                assert_eq!(decoded.srid, srid);
            }
        }
        let nonfinite = Geometry::MultiLineString(MultiLineString(vec![LineString(vec![
            Coord {
                x: f64::NAN,
                y: 0.0,
            },
            Coord { x: 1.0, y: 1.0 },
        ])]));
        assert_eq!(
            encode_wkb(&nonfinite, None).unwrap_err(),
            SpatialError::NonFiniteCoordinate
        );
    }

    #[test]
    fn rejects_unclosed_ring_non_finite_and_giant_geometry() {
        let limits = ValidationLimits {
            max_vertices: 5,
            ..ValidationLimits::default()
        };

        let unclosed = json!({
            "type": "Polygon",
            "coordinates": [[[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]]]
        });
        assert_eq!(
            parse_geojson(&unclosed, &limits).unwrap_err(),
            SpatialError::UnclosedOrShortRing
        );

        let giant = json!({
            "type": "LineString",
            "coordinates": [[0.0, 0.0], [1.0, 0.0], [2.0, 0.0], [3.0, 0.0], [4.0, 0.0], [5.0, 0.0]]
        });
        assert_eq!(
            parse_geojson(&giant, &limits).unwrap_err(),
            SpatialError::VertexLimitExceeded {
                actual: 6,
                limit: 5
            }
        );

        let bad_point = Geometry::Point(Point::new(f64::NAN, 21.0));
        assert_eq!(
            validate_geometry(&bad_point, &limits).unwrap_err(),
            SpatialError::NonFiniteCoordinate
        );
    }
}
