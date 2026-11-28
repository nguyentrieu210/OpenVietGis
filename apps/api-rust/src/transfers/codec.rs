//! E07 pure codecs ported from `adapters/formats/geojson.py` and `domains/transfers/domain.py`.
//!
//! Python-compatible float `repr`, canonical (sorted, ASCII) JSON fingerprints,
//! revision manifests, the RFC 7946 GeoJSON reader, the streamed GeoJSON writer
//! and the export loss/mapping planner.

use std::collections::HashMap;
use std::fmt;
use std::fmt::Write as _;

use ovg_core::crs::reference_transformation;
use ovg_storage::transfers::{PipelineStep, TransferError};
use serde::de::{Deserialize, Deserializer, MapAccess, SeqAccess, Visitor};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

pub const MAX_UPLOAD_BYTES: usize = 25 * 1024 * 1024;
pub const MAX_TRANSFER_FEATURES: usize = 50_000;
pub const MAX_TRANSFER_FIELDS: usize = 100;
pub const MAX_STRING_LEN: usize = 4096;
pub const SUPPORTED_CRS: [i32; 4] = [4326, 3857, 4756, 5899];
const WEB_MERCATOR_MAX: f64 = ovg_core::crs::WEB_MERCATOR_MAX_COORDINATE_METRE;

type Result<T> = std::result::Result<T, TransferError>;

// ----- Python float repr and canonical JSON -----

/// Append Python's `repr(float)` (shortest round-trip, Python exponent rules).
pub fn push_float(out: &mut String, value: f64) {
    if value == 0.0 {
        out.push_str(if value.is_sign_negative() {
            "-0.0"
        } else {
            "0.0"
        });
        return;
    }
    let sci = format!("{value:e}");
    let (mantissa, exponent) = sci.split_once('e').unwrap_or((sci.as_str(), "0"));
    let exponent: i32 = exponent.parse().unwrap_or(0);
    let mantissa = match mantissa.strip_prefix('-') {
        Some(rest) => {
            out.push('-');
            rest
        }
        None => mantissa,
    };
    let digits: String = mantissa.chars().filter(|c| *c != '.').collect();
    if (-4..16).contains(&exponent) {
        let point = exponent + 1;
        if point <= 0 {
            out.push_str("0.");
            for _ in 0..-point {
                out.push('0');
            }
            out.push_str(&digits);
        } else {
            let point = point as usize;
            if point >= digits.len() {
                out.push_str(&digits);
                for _ in digits.len()..point {
                    out.push('0');
                }
                out.push_str(".0");
            } else {
                out.push_str(&digits[..point]);
                out.push('.');
                out.push_str(&digits[point..]);
            }
        }
    } else {
        out.push_str(&digits[..1]);
        if digits.len() > 1 {
            out.push('.');
            out.push_str(&digits[1..]);
        }
        let sign = if exponent < 0 { '-' } else { '+' };
        let _ = write!(out, "e{sign}{:02}", exponent.unsigned_abs());
    }
}

#[must_use]
pub fn py_float(value: f64) -> String {
    let mut out = String::new();
    push_float(&mut out, value);
    out
}

fn push_ascii_str(out: &mut String, text: &str) {
    out.push('"');
    for c in text.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\u{8}' => out.push_str("\\b"),
            '\u{c}' => out.push_str("\\f"),
            ' '..='~' => out.push(c),
            _ => {
                for unit in c.encode_utf16(&mut [0; 2]) {
                    let _ = write!(out, "\\u{unit:04x}");
                }
            }
        }
    }
    out.push('"');
}

/// `json.dumps(sort_keys=True, separators=(",", ":"), ensure_ascii=True)`.
pub fn push_canonical(out: &mut String, value: &Value) {
    match value {
        Value::Null => out.push_str("null"),
        Value::Bool(b) => out.push_str(if *b { "true" } else { "false" }),
        Value::Number(n) => match (n.as_i64(), n.as_u64()) {
            (Some(i), _) => {
                let _ = write!(out, "{i}");
            }
            (None, Some(u)) => {
                let _ = write!(out, "{u}");
            }
            _ => push_float(out, n.as_f64().unwrap_or(0.0)),
        },
        Value::String(s) => push_ascii_str(out, s),
        Value::Array(items) => {
            out.push('[');
            for (i, item) in items.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                push_canonical(out, item);
            }
            out.push(']');
        }
        // serde_json::Map is a BTreeMap here: keys iterate in code point order.
        Value::Object(map) => {
            out.push('{');
            for (i, (key, item)) in map.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                push_ascii_str(out, key);
                out.push(':');
                push_canonical(out, item);
            }
            out.push('}');
        }
    }
}

/// `canonical_fingerprint` / `canonical_manifest_digest`.
#[must_use]
pub fn fingerprint(value: &Value) -> String {
    let mut out = String::new();
    push_canonical(&mut out, value);
    hex(&Sha256::digest(out.as_bytes()))
}

#[must_use]
pub fn hex(bytes: &[u8]) -> String {
    bytes.iter().fold(String::new(), |mut s, b| {
        let _ = write!(s, "{b:02x}");
        s
    })
}

#[must_use]
pub fn sorted_unique(values: &[String]) -> Vec<String> {
    let mut out = values.to_vec();
    out.sort();
    out.dedup();
    out
}

// ----- names -----

/// `sanitize_filename`.
#[must_use]
pub fn sanitize_filename(raw: &str, fallback: &str) -> String {
    let replaced = raw.replace('\\', "/");
    let base = replaced.rsplit('/').next().unwrap_or("").trim();
    let mut cleaned = String::new();
    let mut in_run = false;
    for c in base.chars() {
        if c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-') {
            cleaned.push(c);
            in_run = false;
        } else if !in_run {
            cleaned.push('_');
            in_run = true;
        }
    }
    let cleaned = cleaned.trim_matches(|c| c == '.' || c == '_');
    if cleaned.is_empty() {
        return fallback.to_string();
    }
    cleaned.chars().take(200).collect()
}

fn valid_field_id(id: &str) -> bool {
    let mut chars = id.chars();
    chars.next().is_some_and(|c| c.is_ascii_alphabetic())
        && id.len() <= 64
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// `sanitize_field_id`.
fn sanitize_field_id(raw: &str, used: &mut Vec<String>, ordinal: usize) -> String {
    let mut candidate = String::new();
    let mut in_run = false;
    for c in raw.trim().chars() {
        if c.is_ascii_alphanumeric() || c == '_' {
            candidate.push(c);
            in_run = false;
        } else if !in_run {
            candidate.push('_');
            in_run = true;
        }
    }
    let mut candidate = candidate.trim_matches('_').to_string();
    if !candidate
        .chars()
        .next()
        .is_some_and(|c| c.is_ascii_alphabetic())
    {
        candidate = if candidate.is_empty() {
            format!("fld_{ordinal}")
        } else {
            format!("fld_{candidate}")
        };
    }
    candidate.truncate(60);
    if !valid_field_id(&candidate) {
        candidate = format!("fld_{ordinal}");
    }
    let base = candidate.clone();
    let mut suffix = 1;
    while used.contains(&candidate) {
        suffix += 1;
        candidate = format!("{}_{suffix}", &base[..base.len().min(56)]);
    }
    used.push(candidate.clone());
    candidate
}

// ----- order-preserving JSON (Python json.loads semantics) -----

#[derive(Debug, Clone, PartialEq)]
pub enum J {
    Null,
    Bool(bool),
    Int(i128),
    Float(f64),
    Str(String),
    Arr(Vec<J>),
    Obj(Vec<(String, J)>),
}

impl J {
    fn get(&self, key: &str) -> Option<&J> {
        match self {
            J::Obj(entries) => entries.iter().find(|(k, _)| k == key).map(|(_, v)| v),
            _ => None,
        }
    }

    fn truthy(&self) -> bool {
        match self {
            J::Null => false,
            J::Bool(b) => *b,
            J::Int(i) => *i != 0,
            J::Float(f) => *f != 0.0,
            J::Str(s) => !s.is_empty(),
            J::Arr(a) => !a.is_empty(),
            J::Obj(o) => !o.is_empty(),
        }
    }

    /// Python `str(value)` for scalars.
    fn py_str(&self) -> String {
        match self {
            J::Null => "None".into(),
            J::Bool(true) => "True".into(),
            J::Bool(false) => "False".into(),
            J::Int(i) => i.to_string(),
            J::Float(f) => py_float(*f),
            J::Str(s) => s.clone(),
            // shortcut: containers render as a marker, not a full Python repr; upgrade if clients parse it.
            J::Arr(_) => "[...]".into(),
            J::Obj(_) => "{...}".into(),
        }
    }
}

impl<'de> Deserialize<'de> for J {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> std::result::Result<Self, D::Error> {
        deserializer.deserialize_any(JVisitor)
    }
}

struct JVisitor;

impl<'de> Visitor<'de> for JVisitor {
    type Value = J;

    fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str("a JSON value")
    }
    fn visit_unit<E>(self) -> std::result::Result<J, E> {
        Ok(J::Null)
    }
    fn visit_bool<E>(self, v: bool) -> std::result::Result<J, E> {
        Ok(J::Bool(v))
    }
    fn visit_i64<E>(self, v: i64) -> std::result::Result<J, E> {
        Ok(J::Int(i128::from(v)))
    }
    fn visit_u64<E>(self, v: u64) -> std::result::Result<J, E> {
        Ok(J::Int(i128::from(v)))
    }
    fn visit_f64<E>(self, v: f64) -> std::result::Result<J, E> {
        Ok(J::Float(v))
    }
    fn visit_str<E>(self, v: &str) -> std::result::Result<J, E> {
        Ok(J::Str(v.to_owned()))
    }
    fn visit_string<E>(self, v: String) -> std::result::Result<J, E> {
        Ok(J::Str(v))
    }
    fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> std::result::Result<J, A::Error> {
        let mut items = Vec::new();
        while let Some(item) = seq.next_element()? {
            items.push(item);
        }
        Ok(J::Arr(items))
    }
    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> std::result::Result<J, A::Error> {
        // Python dicts keep the first position of a duplicate key and its last value.
        let mut entries: Vec<(String, J)> = Vec::new();
        let mut index: HashMap<String, usize> = HashMap::new();
        while let Some((key, value)) = map.next_entry::<String, J>()? {
            if let Some(&at) = index.get(&key) {
                entries[at].1 = value;
            } else {
                index.insert(key.clone(), entries.len());
                entries.push((key, value));
            }
        }
        Ok(J::Obj(entries))
    }
}

// ----- geometry -----

#[derive(Debug, Clone, PartialEq)]
pub enum Geom {
    Point([f64; 2]),
    Line(Vec<[f64; 2]>),
    Polygon(Vec<Vec<[f64; 2]>>),
}

/// `_finite`: normalize negative zero only.
fn finite(v: f64) -> Option<f64> {
    v.is_finite().then_some(if v == 0.0 { 0.0 } else { v })
}

impl Geom {
    #[must_use]
    pub fn family(&self) -> &'static str {
        match self {
            Geom::Point(_) => "Point",
            Geom::Line(_) => "LineString",
            Geom::Polygon(_) => "Polygon",
        }
    }

    fn parts(&self) -> Vec<&[[f64; 2]]> {
        match self {
            Geom::Point(p) => vec![std::slice::from_ref(p)],
            Geom::Line(l) => vec![l.as_slice()],
            Geom::Polygon(rings) => rings.iter().map(Vec::as_slice).collect(),
        }
    }

    pub fn positions(&self) -> impl Iterator<Item = [f64; 2]> + '_ {
        self.parts().into_iter().flat_map(|p| p.iter().copied())
    }

    /// Repository `_wkt` (Python float repr keeps the exact value).
    #[must_use]
    pub fn wkt(&self) -> String {
        let seq = |out: &mut String, pts: &[[f64; 2]]| {
            for (i, p) in pts.iter().enumerate() {
                if i > 0 {
                    out.push_str(", ");
                }
                push_float(out, p[0]);
                out.push(' ');
                push_float(out, p[1]);
            }
        };
        let mut out = String::new();
        match self {
            Geom::Point(p) => {
                out.push_str("POINT(");
                seq(&mut out, std::slice::from_ref(p));
            }
            Geom::Line(l) => {
                out.push_str("LINESTRING(");
                seq(&mut out, l);
            }
            Geom::Polygon(rings) => {
                out.push_str("POLYGON(");
                for (i, ring) in rings.iter().enumerate() {
                    if i > 0 {
                        out.push_str(", ");
                    }
                    out.push('(');
                    seq(&mut out, ring);
                    out.push(')');
                }
            }
        }
        out.push(')');
        out
    }

    /// Coordinates as JSON with the given separator (", " for json.dumps defaults).
    pub fn push_coordinates(&self, out: &mut String, sep: &str) {
        let pos = |out: &mut String, p: &[f64; 2]| {
            out.push('[');
            push_float(out, p[0]);
            out.push_str(sep);
            push_float(out, p[1]);
            out.push(']');
        };
        let seq = |out: &mut String, pts: &[[f64; 2]]| {
            out.push('[');
            for (i, p) in pts.iter().enumerate() {
                if i > 0 {
                    out.push_str(sep);
                }
                pos(out, p);
            }
            out.push(']');
        };
        match self {
            Geom::Point(p) => pos(out, p),
            Geom::Line(l) => seq(out, l),
            Geom::Polygon(rings) => {
                out.push('[');
                for (i, ring) in rings.iter().enumerate() {
                    if i > 0 {
                        out.push_str(sep);
                    }
                    seq(out, ring);
                }
                out.push(']');
            }
        }
    }

    pub(super) fn coordinates_value(&self) -> Value {
        let pos = |p: &[f64; 2]| json!([p[0], p[1]]);
        match self {
            Geom::Point(p) => pos(p),
            Geom::Line(l) => Value::Array(l.iter().map(pos).collect()),
            Geom::Polygon(rings) => Value::Array(
                rings
                    .iter()
                    .map(|r| Value::Array(r.iter().map(pos).collect()))
                    .collect(),
            ),
        }
    }
}

/// `_read_native_wkb`: bounded little-endian 2D OGC WKB.
pub fn decode_wkb(value: &[u8]) -> Option<Geom> {
    if value.len() < 5 || value.len() > 5 + 4 + 4096 * 20 || value[0] != 1 {
        return None;
    }
    let mut offset = 5;
    let mut remaining = 4096usize;
    let u32_at = |offset: &mut usize| -> Option<usize> {
        let bytes: [u8; 4] = value.get(*offset..*offset + 4)?.try_into().ok()?;
        *offset += 4;
        usize::try_from(u32::from_le_bytes(bytes)).ok()
    };
    let mut positions = |offset: &mut usize, n: usize| -> Option<Vec<[f64; 2]>> {
        if n < 1 || n > remaining || *offset + n * 16 > value.len() {
            return None;
        }
        let mut out = Vec::with_capacity(n);
        for i in 0..n {
            let at = *offset + i * 16;
            let x = f64::from_le_bytes(value[at..at + 8].try_into().ok()?);
            let y = f64::from_le_bytes(value[at + 8..at + 16].try_into().ok()?);
            out.push([finite(x)?, finite(y)?]);
        }
        *offset += n * 16;
        remaining -= n;
        Some(out)
    };
    let kind = u32::from_le_bytes(value[1..5].try_into().ok()?);
    let geom = match kind {
        1 => Geom::Point(positions(&mut offset, 1)?[0]),
        2 => {
            let n = u32_at(&mut offset)?;
            Geom::Line(positions(&mut offset, n)?)
        }
        3 => {
            let rings = u32_at(&mut offset)?;
            if !(1..=1024).contains(&rings) {
                return None;
            }
            let mut out = Vec::with_capacity(rings);
            for _ in 0..rings {
                let n = u32_at(&mut offset)?;
                out.push(positions(&mut offset, n)?);
            }
            Geom::Polygon(out)
        }
        _ => return None,
    };
    (offset == value.len()).then_some(geom)
}

fn distinct(points: &[[f64; 2]]) -> usize {
    let mut keys: Vec<(u64, u64)> = points
        .iter()
        .map(|p| (p[0].to_bits(), p[1].to_bits()))
        .collect();
    keys.sort_unstable();
    keys.dedup();
    keys.len()
}

/// `validate_ready_geometry`; `false` is a `SpatialValidationError`.
pub fn is_ready(geom: &Geom, family: Option<&str>, srid: i32, output_roundoff: bool) -> bool {
    validate_ready(geom, family, srid, output_roundoff).is_some()
}

fn validate_ready(
    geom: &Geom,
    family: Option<&str>,
    srid: i32,
    output_roundoff: bool,
) -> Option<()> {
    if !SUPPORTED_CRS.contains(&srid) || family.is_some_and(|f| f != geom.family()) {
        return None;
    }
    let parts = geom.parts();
    let mut total = 0;
    for part in &parts {
        total += part.len();
        match geom {
            Geom::Point(_) => {}
            Geom::Line(_) => {
                if part.len() < 2 || distinct(part) < 2 {
                    return None;
                }
            }
            Geom::Polygon(_) => {
                if part.len() < 4
                    || part.first() != part.last()
                    || distinct(&part[..part.len() - 1]) < 3
                {
                    return None;
                }
                let twice_area: f64 = part
                    .windows(2)
                    .map(|w| w[0][0] * w[1][1] - w[1][0] * w[0][1])
                    .sum();
                if !twice_area.is_finite() || twice_area == 0.0 {
                    return None;
                }
            }
        }
    }
    if total > 4096 {
        return None;
    }
    let slack = if output_roundoff { 1e-6 } else { 0.0 };
    for part in parts {
        for &[x, y] in part {
            let ok = match srid {
                4326 | 4756 => (-180.0..=180.0).contains(&x) && (-90.0..=90.0).contains(&y),
                3857 => x.abs() <= WEB_MERCATOR_MAX + slack && y.abs() <= WEB_MERCATOR_MAX + slack,
                _ => true,
            };
            if !ok {
                return None;
            }
        }
        if matches!(srid, 4326 | 4756) && part.windows(2).any(|w| (w[0][0] - w[1][0]).abs() > 180.0)
        {
            return None;
        }
    }
    Some(())
}

// ----- CRS pipelines (domains/spatial/crs.py) -----

const WEB_MERCATOR_PIPELINE: &str =
    "+proj=pipeline +step +proj=unitconvert +xy_in=deg +xy_out=rad +step +proj=webmerc +ellps=WGS84";
const EPSG_6960_PIPELINE: &str = "urn:ogc:def:coordinateOperation:EPSG::6960";
const TM3_PIPELINE: &str = "+proj=pipeline +step +proj=unitconvert +xy_in=deg +xy_out=rad \
     +step +proj=tmerc +lat_0=0 +lon_0=107.75 +k=0.9999 +x_0=500000 +y_0=0 +ellps=WGS84";

const fn step(inverse: bool, pipeline: &'static str, target: i32) -> PipelineStep {
    PipelineStep {
        inverse,
        pipeline,
        target,
    }
}

/// Approved transformation: operation id, pipeline steps and the area-of-use check.
/// No approved direct operation between two CRS codes.
#[derive(Debug)]
pub struct NoRoute;

pub struct Transform {
    pub source: i32,
    pub operation_id: &'static str,
    pub steps: Vec<PipelineStep>,
    /// The extra TM-3 inverse step whose output is area-checked for 5899 targets.
    pub check_steps: usize,
    bounds: ovg_core::crs::CoordinateBounds,
}

impl Transform {
    /// `None` for identity; `Err` when no approved edge exists.
    pub fn resolve(source: i32, target: i32) -> std::result::Result<Option<Self>, NoRoute> {
        if source == target {
            return Ok(None);
        }
        let (Ok(s), Ok(t)) = (u32::try_from(source), u32::try_from(target)) else {
            return Err(NoRoute);
        };
        let profile = reference_transformation(s, t).ok_or(NoRoute)?;
        let web_fwd = step(false, WEB_MERCATOR_PIPELINE, 3857);
        let web_inv = step(true, WEB_MERCATOR_PIPELINE, 4326);
        let datum_fwd = step(false, EPSG_6960_PIPELINE, 4326);
        let datum_inv = step(true, EPSG_6960_PIPELINE, 4756);
        let tm3_fwd = step(false, TM3_PIPELINE, 5899);
        let tm3_inv = step(true, TM3_PIPELINE, 4756);
        let mut steps = match (source, target) {
            (4326, 3857) => vec![web_fwd],
            (3857, 4326) => vec![web_inv],
            (4756, 4326) => vec![datum_fwd],
            (4326, 4756) => vec![datum_inv],
            (5899, 4756) => vec![tm3_inv],
            (4756, 5899) => vec![tm3_fwd],
            (5899, 4326) => vec![tm3_inv, datum_fwd],
            (4326, 5899) => vec![datum_inv, tm3_fwd],
            _ => return Err(NoRoute),
        };
        let check_steps = steps.len();
        if target == 5899 {
            steps.push(tm3_inv);
        }
        Ok(Some(Self {
            source,
            operation_id: profile.operation_id,
            steps,
            check_steps,
            bounds: profile.geographic_bounds,
        }))
    }

    /// Area-of-use + output validation of one feature's step outputs, mirroring
    /// `validate_and_transform_geometries`. Returns the final geometry.
    pub fn apply(
        &self,
        native: &Geom,
        family: &str,
        wkb: &[Option<Vec<u8>>],
        step_valid: &[bool],
    ) -> Result<Geom> {
        let out_of_area = || {
            TransferError::validation(
                "TRANSFORMATION_OUT_OF_AREA",
                "Geometry vertex is outside the approved transformation area of use.",
            )
        };
        let inside = |g: &Geom| g.positions().all(|[x, y]| self.bounds.contains(x, y, 0.0));
        if matches!(self.source, 4326 | 4756) && !inside(native) {
            return Err(out_of_area());
        }
        let mut current = native.clone();
        for (index, step) in self.steps.iter().enumerate() {
            let valid = step_valid.get(index).copied().unwrap_or(false);
            let bytes = wkb.get(index + 1).and_then(Option::as_deref);
            let (true, Some(bytes)) = (valid, bytes) else {
                return Err(TransferError::validation(
                    "TRANSFORMATION_INVALID_OUTPUT",
                    "Coordinate transformation produced invalid geometry.",
                ));
            };
            let decoded = decode_wkb(bytes)
                .filter(|g| is_ready(g, Some(family), step.target, true))
                .ok_or_else(|| TransferError::internal("transformed geometry failed validation"))?;
            if matches!(step.target, 4326 | 4756) && !inside(&decoded) {
                return Err(out_of_area());
            }
            if index < self.check_steps {
                current = decoded;
            }
        }
        Ok(current)
    }
}

// ----- GeoJSON reader -----

#[derive(Debug, Clone)]
pub struct Field {
    pub field_id: String,
    pub name: String,
    pub field_type: &'static str,
    pub nullable: bool,
    pub ordinal: usize,
    pub source_name: String,
}

impl Field {
    fn wire(&self) -> Value {
        json!({
            "field_id": self.field_id,
            "name": self.name,
            "type": self.field_type,
            "nullable": self.nullable,
            "ordinal": self.ordinal,
            "source_name": self.source_name,
        })
    }
}

#[derive(Debug, Clone)]
pub struct Finding {
    pub code: &'static str,
    pub message: String,
    pub feature_index: Option<usize>,
    pub field_name: Option<String>,
}

impl Finding {
    fn new(code: &'static str, message: impl Into<String>, index: Option<usize>) -> Self {
        Self {
            code,
            message: message.into(),
            feature_index: index,
            field_name: None,
        }
    }

    fn wire(&self) -> Value {
        json!({
            "severity": "error",
            "code": self.code,
            "message": self.message,
            "feature_index": self.feature_index,
            "field_name": self.field_name,
        })
    }
}

pub struct Record {
    pub feature_id: String,
    /// Coerced value per field (aligned with `Parsed::fields`).
    pub attrs: Vec<Value>,
    pub geom: Geom,
}

pub struct Parsed {
    pub family: &'static str,
    pub dims: &'static str,
    pub fields: Vec<Field>,
    pub records: Vec<Record>,
    pub findings: Vec<Finding>,
    pub extent: Option<Value>,
}

/// `VectorFormatRegistry.detect_format` (content, never extension).
pub fn detect_format(data: &[u8]) -> Result<&'static str> {
    if data.starts_with(b"SQLite format 3\0") {
        return Ok("geopackage");
    }
    if data.starts_with(b"PK\x03\x04") || data.starts_with(b"PK\x05\x06") {
        return Ok("shapefile_zip");
    }
    let start = data
        .iter()
        .position(|b| !matches!(b, b' ' | b'\t' | b'\n' | b'\r' | 0x0b | 0x0c));
    if start.is_some_and(|i| data[i] == b'{') {
        return Ok("geojson");
    }
    Err(TransferError::validation(
        "UNSUPPORTED_FORMAT",
        "Unrecognized file content; supported formats are GeoPackage, \
         RFC 7946 GeoJSON, and ZIP Shapefile.",
    ))
}

fn load_geojson(data: &[u8]) -> Result<Vec<J>> {
    let text = std::str::from_utf8(data)
        .map_err(|_| TransferError::validation("INVALID_UTF8", "GeoJSON must be valid UTF-8."))?;
    let root: J = serde_json::from_str(text)
        .map_err(|_| TransferError::validation("INVALID_JSON", "Malformed JSON document."))?;
    if root.get("type") != Some(&J::Str("FeatureCollection".into())) {
        return Err(TransferError::validation(
            "UNSUPPORTED_GEOJSON",
            "GeoJSON root must be a FeatureCollection.",
        ));
    }
    if root.get("crs").is_some() {
        return Err(TransferError::validation(
            "LEGACY_GEOJSON_CRS",
            "RFC 7946 forbids legacy 'crs' declarations; supply WGS84 RFC 7946 GeoJSON.",
        ));
    }
    let J::Obj(entries) = root else {
        return Err(TransferError::internal("root object expected"));
    };
    let features = entries
        .into_iter()
        .find(|(k, _)| k == "features")
        .map(|(_, v)| v);
    let Some(J::Arr(features)) = features else {
        return Err(TransferError::validation(
            "INVALID_GEOJSON",
            "FeatureCollection 'features' must be a list.",
        ));
    };
    if features.len() > MAX_TRANSFER_FEATURES {
        return Err(TransferError::validation(
            "FEATURE_LIMIT_EXCEEDED",
            format!("GeoJSON feature count exceeds limit of {MAX_TRANSFER_FEATURES}."),
        ));
    }
    Ok(features)
}

fn number(j: &J) -> Option<f64> {
    match j {
        J::Int(i) => Some(*i as f64),
        J::Float(f) => Some(*f),
        _ => None,
    }
}

/// `_check_coords` -> (is_2d_finite_wgs84, has_non_2d).
fn check_coords(coords: Option<&J>, depth: u8) -> (bool, bool) {
    let Some(J::Arr(items)) = coords else {
        return (false, false);
    };
    if depth == 0 {
        if items.len() < 2 {
            return (false, false);
        }
        if items.len() > 2 {
            return (false, true);
        }
        let (Some(x), Some(y)) = (number(&items[0]), number(&items[1])) else {
            return (false, false);
        };
        let ok = x.is_finite()
            && y.is_finite()
            && (-180.0..=180.0).contains(&x)
            && (-90.0..=90.0).contains(&y);
        return (ok, false);
    }
    if items.is_empty() {
        return (false, false);
    }
    for child in items {
        let (ok, non2d) = check_coords(Some(child), depth - 1);
        if !ok {
            return (false, non2d);
        }
    }
    (true, false)
}

fn to_geom(coords: &J, family: &str) -> Option<Geom> {
    let pos = |j: &J| -> Option<[f64; 2]> {
        let J::Arr(p) = j else { return None };
        Some([finite(number(p.first()?)?)?, finite(number(p.get(1)?)?)?])
    };
    let seq = |j: &J| -> Option<Vec<[f64; 2]>> {
        let J::Arr(items) = j else { return None };
        items.iter().map(pos).collect()
    };
    match family {
        "Point" => Some(Geom::Point(pos(coords)?)),
        "LineString" => Some(Geom::Line(seq(coords)?)),
        "Polygon" => {
            let J::Arr(rings) = coords else { return None };
            Some(Geom::Polygon(rings.iter().map(seq).collect::<Option<_>>()?))
        }
        _ => None,
    }
}

fn digits(s: &str) -> bool {
    !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit())
}

/// `^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}(?:\.\d+)?(?:Z|[+-]\d{2}:\d{2})$` (ASCII digits).
fn is_iso_ts(s: &str) -> bool {
    let b = s.as_bytes();
    if b.len() < 20 {
        return false;
    }
    let fixed = digits(&s[0..4])
        && b[4] == b'-'
        && digits(&s[5..7])
        && b[7] == b'-'
        && digits(&s[8..10])
        && b[10] == b'T'
        && digits(&s[11..13])
        && b[13] == b':'
        && digits(&s[14..16])
        && b[16] == b':'
        && digits(&s[17..19]);
    if !fixed {
        return false;
    }
    let mut rest = &s[19..];
    if let Some(frac) = rest.strip_prefix('.') {
        let n = frac.bytes().take_while(u8::is_ascii_digit).count();
        if n == 0 {
            return false;
        }
        rest = &frac[n..];
    }
    rest == "Z"
        || (rest.len() == 6
            && matches!(rest.as_bytes()[0], b'+' | b'-')
            && digits(&rest[1..3])
            && rest.as_bytes()[3] == b':'
            && digits(&rest[4..6]))
}

/// `^-?\d+\.\d+$`.
fn is_dec_str(s: &str) -> bool {
    let s = s.strip_prefix('-').unwrap_or(s);
    s.split_once('.')
        .is_some_and(|(int, frac)| digits(int) && digits(frac))
}

fn is_feature_id(s: &str) -> bool {
    (1..=128).contains(&s.len())
        && s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
}

/// `_infer_field_type`.
fn infer_field_type(values: &[&J]) -> (&'static str, bool) {
    let nullable = values.iter().any(|v| matches!(v, J::Null));
    let non_null: Vec<&J> = values
        .iter()
        .copied()
        .filter(|v| !matches!(v, J::Null))
        .collect();
    if non_null.is_empty() {
        return ("text", true);
    }
    if non_null.iter().all(|v| matches!(v, J::Bool(_))) {
        return ("boolean", nullable);
    }
    if non_null.iter().all(|v| matches!(v, J::Int(_))) {
        let in_range = non_null
            .iter()
            .all(|v| matches!(v, J::Int(i) if i32::try_from(*i).is_ok()));
        return (if in_range { "integer" } else { "decimal" }, nullable);
    }
    if non_null
        .iter()
        .all(|v| matches!(v, J::Int(_) | J::Float(_)))
    {
        return ("decimal", nullable);
    }
    if non_null.iter().all(|v| matches!(v, J::Str(_))) {
        let strs = || {
            non_null.iter().map(|v| match v {
                J::Str(s) => s.as_str(),
                _ => "",
            })
        };
        if strs().all(is_iso_ts) {
            return ("timestamp", nullable);
        }
        if strs().all(is_dec_str) {
            return ("decimal", nullable);
        }
    }
    ("text", nullable)
}

/// `str(Decimal(text).normalize())` for decimal literals (incl. Python float repr).
#[must_use]
pub fn decimal_normalize(text: &str) -> String {
    let (negative, body) = match text.strip_prefix('-') {
        Some(rest) => (true, rest),
        None => (false, text),
    };
    let (mantissa, exp) = match body.find(['e', 'E']) {
        Some(at) => (&body[..at], body[at + 1..].parse::<i64>().unwrap_or(0)),
        None => (body, 0),
    };
    let (int, frac) = mantissa.split_once('.').unwrap_or((mantissa, ""));
    let mut coeff: String = format!("{int}{frac}").trim_start_matches('0').to_string();
    let mut exponent = exp - frac.len() as i64;
    let sign = if negative { "-" } else { "" };
    if coeff.is_empty() {
        return format!("{sign}0");
    }
    while coeff.ends_with('0') {
        coeff.pop();
        exponent += 1;
    }
    let adjusted = exponent + coeff.len() as i64 - 1;
    if exponent <= 0 && adjusted >= -6 {
        if exponent == 0 {
            return format!("{sign}{coeff}");
        }
        let point = coeff.len() as i64 + exponent;
        if point > 0 {
            let p = point as usize;
            return format!("{sign}{}.{}", &coeff[..p], &coeff[p..]);
        }
        return format!("{sign}0.{}{coeff}", "0".repeat((-point) as usize));
    }
    let tail = if coeff.len() > 1 {
        format!(".{}", &coeff[1..])
    } else {
        String::new()
    };
    format!("{sign}{}{tail}E{adjusted:+}", &coeff[..1])
}

fn valid_timestamp(s: &str) -> bool {
    let wall = chrono::NaiveDateTime::parse_from_str(&s[..19], "%Y-%m-%dT%H:%M:%S").is_ok();
    let offset_ok = match s.rfind(['+', '-']).filter(|&at| at > 19) {
        Some(at) => {
            let hh: u32 = s[at + 1..at + 3].parse().unwrap_or(99);
            let mm: u32 = s[at + 4..at + 6].parse().unwrap_or(99);
            hh < 24 && mm < 60
        }
        None => true,
    };
    wall && offset_ok
}

/// `_coerce_value`; `Err` carries the Python `ValueError` text.
fn coerce(field_type: &str, value: &J) -> std::result::Result<Value, &'static str> {
    if matches!(value, J::Null) {
        return Ok(Value::Null);
    }
    match field_type {
        "boolean" => match value {
            J::Bool(b) => Ok(Value::Bool(*b)),
            _ => Err("expected boolean"),
        },
        "integer" => match value {
            J::Int(i) => i64::try_from(*i)
                .map(Value::from)
                .map_err(|_| "expected integer"),
            _ => Err("expected integer"),
        },
        "decimal" => match value {
            J::Int(i) => Ok(Value::String(decimal_normalize(&i.to_string()))),
            J::Float(f) if f.is_finite() => Ok(Value::String(decimal_normalize(&py_float(*f)))),
            J::Float(_) => Err("non-finite decimal"),
            J::Str(s) => Ok(Value::String(decimal_normalize(s))),
            _ => Err("expected decimal"),
        },
        "timestamp" => match value {
            J::Str(s) if is_iso_ts(s) => {
                if !valid_timestamp(s) {
                    return Err("expected ISO-8601 timestamp");
                }
                Ok(Value::String(format!("{}Z", &s[..19])))
            }
            _ => Err("expected ISO-8601 timestamp"),
        },
        _ => match value {
            J::Arr(_) | J::Obj(_) => Err("nested JSON objects/arrays are not supported attributes"),
            other => {
                let text = other.py_str();
                if text.chars().count() > MAX_STRING_LEN {
                    return Err("text exceeds 4096 characters");
                }
                Ok(Value::String(text))
            }
        },
    }
}

/// `(f.get("properties") or {})` when it is a dict.
fn props_of(feature: &J) -> Option<&[(String, J)]> {
    match feature.get("properties") {
        None => Some(&[]),
        Some(p) if !p.truthy() => Some(&[]),
        Some(J::Obj(entries)) => Some(entries),
        Some(_) => None,
    }
}

/// `parse_geojson`.
pub fn parse_geojson(data: &[u8]) -> Result<Parsed> {
    let raw = load_geojson(data)?;
    let mut findings = Vec::new();
    let mut families: Vec<&'static str> = Vec::new();
    let mut prop_names: Vec<String> = Vec::new();
    let mut seen_names: std::collections::HashSet<&str> = std::collections::HashSet::new();
    let mut has_non_2d = false;

    for (idx, feat) in raw.iter().enumerate() {
        if !matches!(feat, J::Obj(_)) || feat.get("type") != Some(&J::Str("Feature".into())) {
            findings.push(Finding::new(
                "INVALID_FEATURE",
                "Invalid GeoJSON Feature.",
                Some(idx),
            ));
            continue;
        }
        let geom = match feat.get("geometry") {
            Some(g @ J::Obj(_)) => g,
            _ => {
                findings.push(Finding::new(
                    "NULL_GEOMETRY",
                    "Feature geometry is null/missing.",
                    Some(idx),
                ));
                continue;
            }
        };
        let (family, depth) = match geom.get("type") {
            Some(J::Str(t)) if t == "Point" => ("Point", 0),
            Some(J::Str(t)) if t == "LineString" => ("LineString", 1),
            Some(J::Str(t)) if t == "Polygon" => ("Polygon", 2),
            other => {
                let name = other.map_or_else(|| "None".to_string(), J::py_str);
                findings.push(Finding::new(
                    "UNSUPPORTED_GEOMETRY",
                    format!("Geometry type '{name}' is outside supported 2D single-part profile."),
                    Some(idx),
                ));
                continue;
            }
        };
        if !families.contains(&family) {
            families.push(family);
        }
        let (ok, non2d) = check_coords(geom.get("coordinates"), depth);
        if non2d {
            has_non_2d = true;
            findings.push(Finding::new(
                "UNSUPPORTED_DIMENSIONS",
                "Only 2D XY coordinates are supported.",
                Some(idx),
            ));
        } else if !ok {
            findings.push(Finding::new(
                "INVALID_GEOMETRY",
                "Geometry has non-finite or out-of-range WGS84 coordinates.",
                Some(idx),
            ));
        }
        match feat.get("properties") {
            None | Some(J::Null) => {}
            Some(J::Obj(entries)) => {
                for (key, _) in entries {
                    if seen_names.insert(key.as_str()) {
                        prop_names.push(key.clone());
                    }
                }
            }
            Some(_) => {
                findings.push(Finding::new(
                    "INVALID_PROPERTIES",
                    "Properties must be an object.",
                    Some(idx),
                ));
                continue;
            }
        }
    }

    if prop_names.len() > MAX_TRANSFER_FIELDS {
        findings.push(Finding::new(
            "TOO_MANY_FIELDS",
            format!("Field count exceeds limit of {MAX_TRANSFER_FIELDS}."),
            None,
        ));
    }
    if families.len() > 1 {
        findings.push(Finding::new(
            "MIXED_GEOMETRY_FAMILY",
            "Mixed geometry families are not supported.",
            None,
        ));
    }

    let dict_props: Vec<HashMap<&str, &J>> = raw
        .iter()
        .filter(|f| matches!(f, J::Obj(_)))
        .filter_map(props_of)
        .map(|entries| entries.iter().map(|(k, v)| (k.as_str(), v)).collect())
        .collect();
    let mut used = Vec::new();
    let mut fields = Vec::new();
    for (ordinal, raw_name) in prop_names.iter().take(MAX_TRANSFER_FIELDS).enumerate() {
        let values: Vec<&J> = dict_props
            .iter()
            .map(|p| p.get(raw_name.as_str()).copied().unwrap_or(&J::Null))
            .collect();
        let (field_type, nullable) = infer_field_type(&values);
        let field_id = sanitize_field_id(raw_name, &mut used, ordinal);
        let name: String = raw_name.chars().take(120).collect();
        fields.push(Field {
            name: if name.is_empty() {
                field_id.clone()
            } else {
                name
            },
            field_id,
            field_type,
            nullable,
            ordinal,
            source_name: raw_name.clone(),
        });
    }

    let family = match families.as_slice() {
        [one] => *one,
        [] => "Point",
        _ => "mixed",
    };
    let mut records = Vec::new();
    let mut bounds: Option<[f64; 4]> = None;
    if findings.is_empty() {
        let mut seen = std::collections::HashSet::new();
        for (idx, feat) in raw.iter().enumerate() {
            let fallback = format!("feat-{:06}", idx + 1);
            let mut fid = match feat.get("id") {
                Some(id @ (J::Str(_) | J::Int(_) | J::Bool(_))) if is_feature_id(&id.py_str()) => {
                    id.py_str()
                }
                _ => fallback.clone(),
            };
            if seen.contains(&fid) {
                fid = fallback;
            }
            seen.insert(fid.clone());
            let props: HashMap<&str, &J> = props_of(feat)
                .unwrap_or(&[])
                .iter()
                .map(|(k, v)| (k.as_str(), v))
                .collect();
            let mut attrs = Vec::with_capacity(fields.len());
            for field in &fields {
                let value = props
                    .get(field.source_name.as_str())
                    .copied()
                    .unwrap_or(&J::Null);
                match coerce(field.field_type, value) {
                    Ok(v) => attrs.push(v),
                    Err(message) => {
                        findings.push(Finding {
                            code: "INVALID_ATTRIBUTE_VALUE",
                            message: message.to_string(),
                            feature_index: Some(idx),
                            field_name: Some(field.source_name.clone()),
                        });
                        attrs.push(Value::Null);
                    }
                }
            }
            let geom = feat
                .get("geometry")
                .and_then(|g| g.get("coordinates"))
                .and_then(|c| to_geom(c, family))
                .ok_or_else(|| TransferError::internal("validated geometry did not convert"))?;
            for [x, y] in geom.positions() {
                bounds = Some(match bounds {
                    None => [x, y, x, y],
                    Some(b) => [b[0].min(x), b[1].min(y), b[2].max(x), b[3].max(y)],
                });
            }
            records.push(Record {
                feature_id: fid,
                attrs,
                geom,
            });
        }
    }
    Ok(Parsed {
        family,
        dims: if has_non_2d { "XYZ" } else { "XY" },
        fields,
        records,
        findings,
        extent: bounds.map(|b| extent_value(b, 4326)),
    })
}

#[must_use]
pub fn extent_value(b: [f64; 4], crs: i32) -> Value {
    json!({"min_x": b[0], "min_y": b[1], "max_x": b[2], "max_y": b[3], "crs_code": crs})
}

// ----- inspection report -----

pub struct Conversion {
    pub code: &'static str,
    pub message: String,
    pub requires_approval: bool,
    pub affected_fields: Vec<String>,
}

impl Conversion {
    fn new(code: &'static str, message: impl Into<String>, approval: bool) -> Self {
        Self {
            code,
            message: message.into(),
            requires_approval: approval,
            affected_fields: Vec::new(),
        }
    }

    #[must_use]
    pub fn wire(&self) -> Value {
        json!({
            "code": self.code,
            "message": self.message,
            "requires_approval": self.requires_approval,
            "affected_fields": self.affected_fields,
        })
    }
}

pub struct InspectOptions<'a> {
    pub asset_id: uuid::Uuid,
    pub asset_sha256: &'a str,
    pub selected_member: Option<&'a str>,
    pub declared_source_crs: Option<i64>,
    pub target_crs: Option<i64>,
    pub declared_encoding: Option<&'a str>,
}

pub struct Inspection {
    pub wire: Value,
    pub fingerprint: String,
    pub confirmable: bool,
    pub first_error: Option<(&'static str, String)>,
    pub conversions: Vec<Conversion>,
    pub target_crs: Option<i64>,
    pub resolved_encoding: Option<String>,
    pub selected_member: Option<String>,
    pub parsed: Option<Parsed>,
}

/// `normalize_encoding`.
pub(super) fn normalize_encoding(raw: &str) -> Option<&'static str> {
    match raw.trim().to_lowercase().replace('_', "-").as_str() {
        "utf8" | "utf-8" | "utf-8-sig" => Some("utf-8"),
        "windows-1258" | "cp1258" | "1258" => Some("windows-1258"),
        "windows-1252" | "cp1252" | "1252" => Some("cp1252"),
        "iso-8859-1" | "latin1" | "latin-1" => Some("latin-1"),
        _ => None,
    }
}

/// `TransferService._build_inspection` for GeoJSON assets.
pub fn build_geojson_inspection(payload: &[u8], opts: &InspectOptions<'_>) -> Result<Inspection> {
    let normalized = match opts.declared_encoding {
        Some(raw) => Some(normalize_encoding(raw).ok_or_else(|| {
            TransferError::validation(
                "UNSUPPORTED_ENCODING",
                "Declared text encoding is not in the accepted encoding profile.",
            )
        })?),
        None => None,
    };
    let parsed = parse_geojson(payload)?;
    let eligible = parsed.findings.is_empty();
    let member_wire = json!({
        "name": "features",
        "eligible": eligible,
        "reason": parsed.findings.first().map(|f| f.message.clone()),
        "geometry_family": parsed.family,
        "dimensions": parsed.dims,
        "feature_count": parsed.records.len(),
        "detected_crs": 4326,
        "detected_encoding": "utf-8",
        "fields": parsed.fields.iter().map(Field::wire).collect::<Vec<_>>(),
    });
    let chosen = match opts.selected_member {
        Some("features") => Some("features"),
        Some(_) => {
            return Err(TransferError::validation(
                "UNKNOWN_MEMBER",
                "Selected dataset member does not exist.",
            ))
        }
        None if eligible => Some("features"),
        None => None,
    };

    let mut findings: Vec<Finding> = Vec::new();
    let mut conversions = Vec::new();
    let mut resolved_source = opts.declared_source_crs;
    let mut resolved_target = opts.target_crs;
    let mut resolved_encoding = normalized.map(str::to_string);
    let mut used = None;
    let (mut count, mut family, mut dims, mut extent, mut fields) =
        (0, Value::Null, Value::Null, Value::Null, Vec::new());
    if chosen.is_some() {
        if !eligible {
            findings.push(Finding::new(
                "INELIGIBLE_MEMBER",
                parsed.findings[0].message.clone(),
                None,
            ));
        } else {
            count = parsed.records.len();
            family = json!(parsed.family);
            dims = json!(parsed.dims);
            extent = parsed.extent.clone().unwrap_or(Value::Null);
            fields = parsed.fields.iter().map(Field::wire).collect();
            resolved_encoding = Some("utf-8".into());
            if let Some(declared) = opts.declared_source_crs.filter(|d| *d != 4326) {
                findings.push(Finding::new(
                    "CRS_DECLARATION_CONFLICT",
                    format!("Declared CRS EPSG:{declared} conflicts with file CRS EPSG:4326."),
                    None,
                ));
            }
            resolved_source = Some(4326);
            let target = opts.target_crs.filter(|t| *t != 0).unwrap_or(4326);
            resolved_target = Some(target);
            match i32::try_from(target)
                .ok()
                .filter(|t| SUPPORTED_CRS.contains(t))
            {
                None => findings.push(Finding::new(
                    "UNSUPPORTED_TARGET_CRS",
                    format!("Target CRS EPSG:{target} is unsupported."),
                    None,
                )),
                Some(4326) => {}
                Some(t) => match Transform::resolve(4326, t) {
                    Ok(Some(profile)) => conversions.push(Conversion::new(
                        "CRS_TRANSFORMATION",
                        format!(
                            "Transform coordinates from EPSG:4326 to EPSG:{t} ({}).",
                            profile.operation_id
                        ),
                        false,
                    )),
                    _ => findings.push(Finding::new(
                        "UNSUPPORTED_TRANSFORMATION",
                        format!("No verified transformation from EPSG:4326 to EPSG:{t}."),
                        None,
                    )),
                },
            }
            let renamed: Vec<String> = parsed
                .fields
                .iter()
                .filter(|f| f.source_name != f.field_id)
                .map(|f| format!("{}->{}", f.source_name, f.field_id))
                .collect();
            if !renamed.is_empty() {
                let mut c = Conversion::new(
                    "FIELD_ID_NORMALIZATION",
                    "Source field names mapped to canonical stable field identities.",
                    false,
                );
                c.affected_fields = renamed;
                conversions.push(c);
            }
            used = Some(());
        }
    } else {
        findings.push(Finding::new(
            "NO_ELIGIBLE_MEMBERS",
            "Dataset contains no supported vector layer members.",
            None,
        ));
    }
    let all_findings: Vec<Finding> = if used.is_some() {
        parsed.findings.iter().cloned().chain(findings).collect()
    } else {
        findings
    };
    let confirmable = chosen.is_some()
        && resolved_source.is_some()
        && resolved_target.is_some()
        && all_findings.is_empty();
    let fp = fingerprint(&json!({
        "kind": "import",
        "asset_id": opts.asset_id.to_string(),
        "asset_sha256": opts.asset_sha256,
        "format": "geojson",
        "member": chosen,
        "source_crs": resolved_source,
        "target_crs": resolved_target,
        "encoding": resolved_encoding,
    }));
    let wire = json!({
        "asset_sha256": opts.asset_sha256,
        "detected_format": "geojson",
        "members": [member_wire],
        "selected_member": chosen,
        "requires_member_selection": false,
        "requires_crs_resolution": false,
        "requires_encoding_resolution": false,
        "resolved_source_crs": resolved_source,
        "target_crs": resolved_target,
        "resolved_encoding": resolved_encoding,
        "sample_only": false,
        "feature_count": count,
        "geometry_family": family,
        "dimensions": dims,
        "extent": extent,
        "fields": fields,
        "findings": all_findings.iter().map(Finding::wire).collect::<Vec<_>>(),
        "conversions": conversions.iter().map(Conversion::wire).collect::<Vec<_>>(),
        "confirmable": confirmable,
        "inspection_fingerprint": fp,
    });
    Ok(Inspection {
        wire,
        fingerprint: fp,
        confirmable,
        first_error: all_findings.first().map(|f| (f.code, f.message.clone())),
        conversions,
        target_crs: resolved_target,
        resolved_encoding,
        selected_member: chosen.map(str::to_string),
        parsed: used.map(|()| parsed),
    })
}

/// Convert the trusted format adapter's typed metadata into a Rust-owned
/// inspection report. Coordinate policy, selection, fingerprints and approval
/// are identical to the GeoJSON path; the decoder has no storage authority.
pub(super) fn build_binary_inspection(
    decoded: Value,
    format: &str,
    opts: &InspectOptions<'_>,
) -> Result<Inspection> {
    fn invalid_report() -> TransferError {
        TransferError::internal("invalid binary format inspection report")
    }
    let members = decoded["members"].as_array().ok_or_else(invalid_report)?;
    let eligible: Vec<&Value> = members
        .iter()
        .filter(|member| member["eligible"].as_bool() == Some(true))
        .collect();
    let selected = if let Some(name) = opts.selected_member {
        if !members.iter().any(|m| m["name"].as_str() == Some(name)) {
            return Err(TransferError::validation(
                "UNKNOWN_MEMBER",
                "Selected dataset member does not exist.",
            ));
        }
        Some(name.to_owned())
    } else if eligible.len() == 1 {
        Some(
            eligible[0]["name"]
                .as_str()
                .ok_or_else(invalid_report)?
                .to_owned(),
        )
    } else {
        None
    };
    let requires_selection = selected.is_none() && eligible.len() > 1;
    let mut findings: Vec<Value> = Vec::new();
    let mut first_error = None;
    let mut conversions = Vec::new();
    let mut source = opts.declared_source_crs;
    let mut target = opts.target_crs;
    let mut encoding = opts
        .declared_encoding
        .and_then(normalize_encoding)
        .map(str::to_owned);
    let mut requires_crs = false;
    let mut requires_encoding = false;
    let mut count = 0;
    let mut family = Value::Null;
    let mut dims = Value::Null;
    let mut extent = Value::Null;
    let mut fields = Vec::new();
    let mut parsed_result = None;

    // The adapter returns findings as wire records. Preserve them and use
    // a static allowlist for the error envelope (no leaking dynamic strings).
    fn finding_code(code: &str) -> &'static str {
        match code {
            "ARCHIVE_BOMB_REJECTED" => "ARCHIVE_BOMB_REJECTED",
            "ARCHIVE_DUPLICATE_ENTRY" => "ARCHIVE_DUPLICATE_ENTRY",
            "ARCHIVE_ENCRYPTED" => "ARCHIVE_ENCRYPTED",
            "ARCHIVE_ENTRY_LIMIT_EXCEEDED" => "ARCHIVE_ENTRY_LIMIT_EXCEEDED",
            "ARCHIVE_PATH_TRAVERSAL" => "ARCHIVE_PATH_TRAVERSAL",
            "ARCHIVE_SIZE_EXCEEDED" => "ARCHIVE_SIZE_EXCEEDED",
            "ARCHIVE_SYMLINK_REJECTED" => "ARCHIVE_SYMLINK_REJECTED",
            "CRS_MISMATCH" => "CRS_MISMATCH",
            "ENCODING_CONFLICT" => "ENCODING_CONFLICT",
            "ENCODING_DECODE_FAILED" => "ENCODING_DECODE_FAILED",
            "FEATURE_LIMIT_EXCEEDED" => "FEATURE_LIMIT_EXCEEDED",
            "INVALID_ATTRIBUTE_VALUE" => "INVALID_ATTRIBUTE_VALUE",
            "INVALID_GEOMETRY" => "INVALID_GEOMETRY",
            "INVALID_GEOPACKAGE" => "INVALID_GEOPACKAGE",
            "INVALID_ZIP" => "INVALID_ZIP",
            "MISSING_SHAPEFILE_COMPONENTS" => "MISSING_SHAPEFILE_COMPONENTS",
            "MIXED_GEOMETRY_FAMILY" => "MIXED_GEOMETRY_FAMILY",
            "NULL_GEOMETRY" => "NULL_GEOMETRY",
            "TOO_MANY_FIELDS" => "TOO_MANY_FIELDS",
            "UNKNOWN_MEMBER" => "UNKNOWN_MEMBER",
            "UNSUPPORTED_DIMENSIONS" => "UNSUPPORTED_DIMENSIONS",
            "UNSUPPORTED_GEOMETRY" => "UNSUPPORTED_GEOMETRY",
            _ => "IMPORT_NOT_CONFIRMABLE",
        }
    }
    fn add_finding(
        findings: &mut Vec<Value>,
        first_error: &mut Option<(&'static str, String)>,
        code: &'static str,
        message: impl Into<String>,
    ) {
        let message = message.into();
        if first_error.is_none() {
            *first_error = Some((code, message.clone()));
        }
        findings.push(Finding::new(code, message, None).wire());
    }

    if let Some(name) = selected.as_deref() {
        let member = members
            .iter()
            .find(|m| m["name"].as_str() == Some(name))
            .ok_or_else(invalid_report)?;
        if member["eligible"].as_bool() != Some(true) {
            add_finding(
                &mut findings,
                &mut first_error,
                "INELIGIBLE_MEMBER",
                member["reason"]
                    .as_str()
                    .unwrap_or("Selected member is not supported."),
            );
        } else {
            let data = &decoded["parsed"];
            if data["member_name"].as_str() != Some(name) {
                return Err(invalid_report());
            }
            let features = data["features"].as_array().ok_or_else(invalid_report)?;
            let wire_fields = data["fields"].as_array().ok_or_else(invalid_report)?;
            if features.len() > MAX_TRANSFER_FEATURES || wire_fields.len() > MAX_TRANSFER_FIELDS {
                return Err(TransferError::coded(
                    413,
                    "FEATURE_LIMIT_EXCEEDED",
                    "Binary dataset exceeds the safe transfer limits.",
                ));
            }
            count = features.len();
            family = data["geometry_family"].clone();
            dims = data["dimensions"].clone();
            extent = data["extent"].clone();
            fields.clone_from(wire_fields);
            encoding = data["detected_encoding"].as_str().map(str::to_owned);
            requires_encoding = data["requires_encoding_resolution"]
                .as_bool()
                .ok_or_else(invalid_report)?;
            for finding in data["findings"].as_array().ok_or_else(invalid_report)? {
                if first_error.is_none() && finding["severity"].as_str() == Some("error") {
                    first_error = Some((
                        finding_code(finding["code"].as_str().unwrap_or("")),
                        finding["message"].as_str().ok_or_else(invalid_report)?.to_owned(),
                    ));
                }
                findings.push(finding.clone());
            }
            if let Some(detected) = data["detected_crs"].as_i64() {
                if opts.declared_source_crs.is_some_and(|declared| declared != detected) {
                    add_finding(
                        &mut findings,
                        &mut first_error,
                        "CRS_DECLARATION_CONFLICT",
                        format!(
                            "Declared CRS EPSG:{} conflicts with file CRS EPSG:{detected}.",
                            opts.declared_source_crs.unwrap_or_default()
                        ),
                    );
                }
                source = Some(detected);
            } else if let Some(declared) = opts.declared_source_crs {
                if !i32::try_from(declared).is_ok_and(|c| SUPPORTED_CRS.contains(&c)) {
                    add_finding(
                        &mut findings,
                        &mut first_error,
                        "UNSUPPORTED_CRS",
                        format!("Declared CRS EPSG:{declared} is unsupported."),
                    );
                } else {
                    conversions.push(Conversion::new(
                        "EXPLICIT_CRS_ASSIGNMENT",
                        format!("Missing file CRS resolved explicitly to EPSG:{declared}."),
                        false,
                    ));
                }
            } else {
                requires_crs = true;
                add_finding(
                    &mut findings,
                    &mut first_error,
                    "MISSING_CRS",
                    "Source CRS is missing or unrecognized; select an approved EPSG profile.",
                );
            }
            if requires_encoding {
                add_finding(
                    &mut findings,
                    &mut first_error,
                    "AMBIGUOUS_ENCODING",
                    "Text encoding is ambiguous; select an approved encoding explicitly.",
                );
            }
            if let Some(source_code) = source {
                let target_code = opts.target_crs.filter(|t| *t != 0).unwrap_or(source_code);
                target = Some(target_code);
                let stored = i32::try_from(source_code).unwrap_or_default();
                let dest = i32::try_from(target_code).unwrap_or_default();
                if !SUPPORTED_CRS.contains(&dest) {
                    add_finding(
                        &mut findings,
                        &mut first_error,
                        "UNSUPPORTED_TARGET_CRS",
                        format!("Target CRS EPSG:{target_code} is unsupported."),
                    );
                } else if stored != dest {
                    match Transform::resolve(stored, dest) {
                        Ok(Some(profile)) => conversions.push(Conversion::new(
                            "CRS_TRANSFORMATION",
                            format!(
                                "Transform coordinates from EPSG:{stored} to EPSG:{dest} ({}).",
                                profile.operation_id
                            ),
                            false,
                        )),
                        _ => add_finding(
                            &mut findings,
                            &mut first_error,
                            "UNSUPPORTED_TRANSFORMATION",
                            format!("No verified transformation from EPSG:{stored} to EPSG:{dest}."),
                        ),
                    }
                }
                let renamed: Vec<String> = wire_fields
                    .iter()
                    .filter_map(|field| {
                        let original = field["source_name"].as_str()?;
                        let canonical = field["field_id"].as_str()?;
                        (original != canonical).then(|| format!("{original}->{canonical}"))
                    })
                    .collect();
                if !renamed.is_empty() {
                    let mut conversion = Conversion::new(
                        "FIELD_ID_NORMALIZATION",
                        "Source field names mapped to canonical stable field identities.",
                        false,
                    );
                    conversion.affected_fields = renamed;
                    conversions.push(conversion);
                }
            }
            if first_error.is_none() && !requires_crs && !requires_encoding {
                let geometry_family = match family.as_str() {
                    Some("Point") => "Point",
                    Some("LineString") => "LineString",
                    Some("Polygon") => "Polygon",
                    _ => return Err(invalid_report()),
                };
                if dims.as_str() != Some("XY") {
                    return Err(TransferError::validation(
                        "UNSUPPORTED_DIMENSIONS",
                        "Only XY geometry is supported.",
                    ));
                }
                let mut typed_fields = Vec::with_capacity(wire_fields.len());
                for field in wire_fields {
                    let text = |key: &str| {
                        field[key].as_str().map(str::to_owned).ok_or_else(invalid_report)
                    };
                    let field_type = match field["type"].as_str() {
                        Some("text") => "text",
                        Some("integer") => "integer",
                        Some("decimal") => "decimal",
                        Some("boolean") => "boolean",
                        Some("timestamp") => "timestamp",
                        _ => return Err(invalid_report()),
                    };
                    typed_fields.push(Field {
                        field_id: text("field_id")?,
                        name: text("name")?,
                        source_name: text("source_name")?,
                        field_type,
                        nullable: field["nullable"].as_bool().ok_or_else(invalid_report)?,
                        ordinal: field["ordinal"]
                            .as_u64()
                            .and_then(|n| usize::try_from(n).ok())
                            .ok_or_else(invalid_report)?,
                    });
                }
                let mut records = Vec::with_capacity(features.len());
                for feature in features {
                    let geometry = &feature["geometry"];
                    if geometry["type"].as_str() != Some(geometry_family) {
                        return Err(invalid_report());
                    }
                    let coordinates: J = serde_json::from_value(geometry["coordinates"].clone())
                        .map_err(|_| invalid_report())?;
                    let geom = to_geom(&coordinates, geometry_family).ok_or_else(invalid_report)?;
                    let attributes = feature["attributes"].as_object().ok_or_else(invalid_report)?;
                    records.push(Record {
                        feature_id: feature["feature_id"]
                            .as_str()
                            .ok_or_else(invalid_report)?
                            .to_owned(),
                        attrs: typed_fields
                            .iter()
                            .map(|f| attributes.get(&f.field_id).cloned().unwrap_or(Value::Null))
                            .collect(),
                        geom,
                    });
                }
                parsed_result = Some(Parsed {
                    family: geometry_family,
                    dims: "XY",
                    fields: typed_fields,
                    records,
                    findings: Vec::new(),
                    extent: (!extent.is_null()).then(|| extent.clone()),
                });
            }
        }
    } else if eligible.is_empty() {
        add_finding(
            &mut findings,
            &mut first_error,
            "NO_ELIGIBLE_MEMBERS",
            "Dataset contains no supported vector layer members.",
        );
    }
    let confirmable = selected.is_some()
        && !requires_selection
        && !requires_crs
        && !requires_encoding
        && source.is_some()
        && target.is_some()
        && first_error.is_none()
        && parsed_result.is_some();
    let fp = fingerprint(&json!({
        "kind": "import",
        "asset_id": opts.asset_id.to_string(),
        "asset_sha256": opts.asset_sha256,
        "format": format,
        "member": selected,
        "source_crs": source,
        "target_crs": target,
        "encoding": encoding,
    }));
    let wire = json!({
        "asset_sha256": opts.asset_sha256,
        "detected_format": format,
        "members": members,
        "selected_member": selected,
        "requires_member_selection": requires_selection,
        "requires_crs_resolution": requires_crs,
        "requires_encoding_resolution": requires_encoding,
        "resolved_source_crs": source,
        "target_crs": target,
        "resolved_encoding": encoding,
        "sample_only": false,
        "feature_count": count,
        "geometry_family": family,
        "dimensions": dims,
        "extent": extent,
        "fields": fields,
        "findings": findings,
        "conversions": conversions.iter().map(Conversion::wire).collect::<Vec<_>>(),
        "confirmable": confirmable,
        "inspection_fingerprint": fp,
    });
    Ok(Inspection {
        wire,
        fingerprint: fp,
        confirmable,
        first_error,
        conversions,
        target_crs: target,
        resolved_encoding: encoding,
        selected_member: selected,
        parsed: parsed_result,
    })
}

// ----- canonical attribute values and revision manifests -----

/// `_canonical_decimal` over the coerced string.
fn canonical_decimal(value: &str) -> Option<String> {
    let body = value.strip_prefix('-').unwrap_or(value);
    let negative = body.len() != value.len();
    let (int, frac) = body.split_once('.').unwrap_or((body, ""));
    if value.len() > 4096 || !digits(int) || (body.contains('.') && !digits(frac)) {
        return None;
    }
    let int = int.trim_start_matches('0');
    let int = if int.is_empty() { "0" } else { int };
    let frac = frac.trim_end_matches('0');
    if int.trim_start_matches('0').len() + frac.len() > 38 || frac.len() > 18 {
        return None;
    }
    if int == "0" && frac.is_empty() {
        return Some("0".into());
    }
    let sign = if negative { "-" } else { "" };
    Some(if frac.is_empty() {
        format!("{sign}{int}")
    } else {
        format!("{sign}{int}.{frac}")
    })
}

/// `canonical_attribute_value`; `None` is a `SpatialValidationError`.
#[must_use]
pub fn canonical_attribute(field: &Field, value: &Value) -> Option<Value> {
    match value {
        Value::Null => field.nullable.then_some(Value::Null),
        Value::String(s) => match field.field_type {
            "text" => {
                (s.chars().count() <= MAX_STRING_LEN && !s.contains('\0')).then(|| value.clone())
            }
            "decimal" => canonical_decimal(s).map(Value::String),
            // Coerced timestamps are already `YYYY-MM-DDTHH:MM:SSZ`.
            "timestamp" => Some(value.clone()),
            _ => None,
        },
        Value::Number(n) if field.field_type == "integer" => n
            .as_i64()
            .filter(|i| i32::try_from(*i).is_ok())
            .map(Value::from),
        Value::Bool(_) if field.field_type == "boolean" => Some(value.clone()),
        _ => None,
    }
}

/// `schema_revision_digest` (field names already trimmed).
#[must_use]
pub fn schema_digest(fields: &[Field], family: &str) -> String {
    let mut ordered: Vec<&Field> = fields.iter().collect();
    ordered.sort_by(|a, b| a.field_id.cmp(&b.field_id));
    fingerprint(&json!({
        "manifest_type": "r003-schema-v1",
        "schema_state": "known",
        "geometry_family": family,
        "dimensions": "XY",
        "fields": ordered.iter().map(|f| json!({
            "field_id": f.field_id,
            "name": f.name,
            "ordinal": f.ordinal,
            "type": f.field_type,
            "nullable": f.nullable,
        })).collect::<Vec<_>>(),
    }))
}

/// `content_revision_digest` over (feature_id, canonical attributes, geometry).
#[must_use]
pub fn content_digest(
    fields: &[Field],
    features: &[(String, serde_json::Map<String, Value>, Geom)],
    source_crs: i32,
    stored_crs: i32,
    family: &str,
) -> String {
    let mut ordered_fields: Vec<&Field> = fields.iter().collect();
    ordered_fields.sort_by(|a, b| a.field_id.cmp(&b.field_id));
    let mut ordered: Vec<&(String, serde_json::Map<String, Value>, Geom)> =
        features.iter().collect();
    ordered.sort_by(|a, b| a.0.cmp(&b.0));
    fingerprint(&json!({
        "manifest_type": "r003-content-v1",
        "source_crs": source_crs,
        "stored_crs": stored_crs,
        "geometry_family": family,
        "dimensions": "XY",
        "field_interpretations": ordered_fields.iter().map(|f| json!({
            "field_id": f.field_id, "type": f.field_type, "nullable": f.nullable,
        })).collect::<Vec<_>>(),
        "features": ordered.iter().map(|(id, attrs, geom)| json!({
            "feature_id": id,
            "attributes": attrs,
            "geometry": {
                "type": geom.family(),
                "dimensions": "XY",
                "srid": stored_crs,
                "coordinates": geom.coordinates_value(),
                "members": [],
            },
            "geometry_state": "present",
        })).collect::<Vec<_>>(),
    }))
}

// ----- export planning and GeoJSON writer -----

pub struct Mapping {
    pub field_id: String,
    pub source_name: String,
    pub output_name: String,
    pub field_type: String,
}

impl Mapping {
    #[must_use]
    pub fn wire(&self) -> Value {
        json!({
            "field_id": self.field_id,
            "source_name": self.source_name,
            "output_name": self.output_name,
            "type": self.field_type,
        })
    }
}

/// `(field_id, name, type)` of the pinned source schema in ordinal order.
pub type SchemaField<'a> = (&'a str, &'a str, &'a str);
pub type ExportMappingPlan = (Vec<Mapping>, Vec<Conversion>, Option<&'static str>);

/// `plan_export_mapping`; `Err` is the `ValueError` text.
pub fn plan_export_mapping(
    fields: &[SchemaField<'_>],
    output_format: &str,
    source_crs: i32,
    output_crs: i32,
    has_long_text: bool,
) -> std::result::Result<ExportMappingPlan, String> {
    if !SUPPORTED_CRS.contains(&source_crs) || !SUPPORTED_CRS.contains(&output_crs) {
        return Err("Unsupported CRS code".into());
    }
    let op_id = Transform::resolve(source_crs, output_crs)
        .map_err(|_| {
            format!("No approved coordinate operation from EPSG:{source_crs} to EPSG:{output_crs}")
        })?
        .map(|t| t.operation_id);
    if output_format == "geojson" && output_crs != 4326 {
        return Err("RFC 7946 GeoJSON export requires EPSG:4326 output CRS".into());
    }
    let mut disclosures = Vec::new();
    if let Some(op) = op_id {
        disclosures.push(Conversion::new(
            "CRS_TRANSFORMATION",
            format!(
                "Coordinates will be transformed from EPSG:{source_crs} to EPSG:{output_crs} \
                 via approved operation {op}."
            ),
            false,
        ));
    }
    let identity = |f: &SchemaField<'_>| Mapping {
        field_id: f.0.to_string(),
        source_name: f.1.to_string(),
        output_name: f.0.to_string(),
        field_type: f.2.to_string(),
    };
    if output_format == "geopackage" {
        return Ok((fields.iter().map(identity).collect(), disclosures, op_id));
    }
    if output_format == "geojson" {
        let stringified: Vec<String> = fields
            .iter()
            .filter(|f| matches!(f.2, "decimal" | "timestamp"))
            .map(|f| f.0.to_string())
            .collect();
        if !stringified.is_empty() {
            let mut c = Conversion::new(
                "GEOJSON_TYPED_STRING_ENCODING",
                "RFC 7946 GeoJSON encodes decimal and timestamp fields as \
                 exact JSON strings rather than distinct native types.",
                true,
            );
            c.affected_fields = stringified;
            disclosures.push(c);
        }
        return Ok((fields.iter().map(identity).collect(), disclosures, op_id));
    }
    let mut used = vec!["OVG_FID".to_string()];
    let (mut mappings, mut truncated, mut timestamps, mut decimals) =
        (Vec::new(), Vec::new(), Vec::new(), Vec::new());
    for f in fields {
        let mut clean: String =
            f.0.chars()
                .map(|c| {
                    if c.is_ascii_alphanumeric() || c == '_' {
                        c
                    } else {
                        '_'
                    }
                })
                .collect::<String>()
                .to_uppercase();
        if !clean
            .chars()
            .next()
            .is_some_and(|c| c.is_ascii_alphabetic())
        {
            clean = format!("F_{clean}");
        }
        let mut candidate: String = clean.chars().take(10).collect();
        let mut idx = 1;
        while used.contains(&candidate) {
            let suffix = format!("_{idx}");
            candidate = clean.chars().take(10 - suffix.len()).collect::<String>() + &suffix;
            idx += 1;
        }
        used.push(candidate.clone());
        if candidate.to_lowercase() != f.0.to_lowercase() {
            truncated.push(format!("{}->{candidate}", f.0));
        }
        if f.2 == "timestamp" {
            timestamps.push(f.0.to_string());
        }
        if f.2 == "decimal" {
            decimals.push(f.0.to_string());
        }
        mappings.push(Mapping {
            output_name: candidate,
            ..identity(f)
        });
    }
    let mut add = |code, message: &str, affected: Vec<String>| {
        if !affected.is_empty() {
            let mut c = Conversion::new(code, message, true);
            c.affected_fields = affected;
            disclosures.push(c);
        }
    };
    add(
        "DBF_FIELD_NAME_TRUNCATED",
        "Shapefile DBF limits field names to 10 ASCII characters; \
         deterministic mapping will be applied.",
        truncated,
    );
    add(
        "DBF_TIMESTAMP_AS_TEXT",
        "Shapefile DBF has no ISO-8601 UTC timestamp type; \
         timestamps will be written as UTC ISO-8601 strings.",
        timestamps,
    );
    add(
        "DBF_DECIMAL_FIXED_PRECISION",
        "Shapefile DBF encodes decimals as fixed-width numeric fields \
         (up to 6 decimal places).",
        decimals,
    );
    if has_long_text {
        let mut c = Conversion::new(
            "DBF_TEXT_TRUNCATED_254",
            "Shapefile DBF truncates character values exceeding 254 bytes.",
            true,
        );
        c.affected_fields = fields
            .iter()
            .filter(|f| f.2 == "text")
            .map(|f| f.0.to_string())
            .collect();
        disclosures.push(c);
    }
    Ok((mappings, disclosures, op_id))
}

pub const GEOJSON_HEAD: &str = "{\"type\": \"FeatureCollection\", \"features\": [";
pub const GEOJSON_TAIL: &str = "]}\n";

/// One `write_geojson` feature exactly as `json.dumps(ensure_ascii=False)` renders it.
pub fn push_geojson_feature(
    out: &mut String,
    feature_id: &str,
    geom: &Geom,
    mappings: &[Mapping],
    attributes: &Value,
) -> Result<()> {
    let string = |s: &str| serde_json::to_string(s).unwrap_or_default();
    out.push_str("{\"type\": \"Feature\", \"id\": ");
    out.push_str(&string(feature_id));
    out.push_str(", \"geometry\": {\"type\": \"");
    out.push_str(geom.family());
    out.push_str("\", \"coordinates\": ");
    geom.push_coordinates(out, ", ");
    out.push_str("}, \"properties\": {");
    for (i, m) in mappings.iter().enumerate() {
        if i > 0 {
            out.push_str(", ");
        }
        out.push_str(&string(&m.output_name));
        out.push_str(": ");
        match attributes.get(&m.field_id) {
            None | Some(Value::Null) => out.push_str("null"),
            Some(Value::Bool(b)) => out.push_str(if *b { "true" } else { "false" }),
            Some(Value::String(s)) => out.push_str(&string(s)),
            Some(Value::Number(n)) if n.as_i64().is_some_and(|i| i32::try_from(i).is_ok()) => {
                let _ = write!(out, "{n}");
            }
            Some(_) => {
                return Err(TransferError::internal(
                    "stored attribute value is not an exact JSON representation",
                ))
            }
        }
    }
    out.push_str("}}");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn float_repr_matches_python() {
        for (value, expected) in [
            (0.0, "0.0"),
            (-0.0, "-0.0"),
            (1.0, "1.0"),
            (105.0, "105.0"),
            (0.1, "0.1"),
            (0.0001, "0.0001"),
            (0.00001, "1e-05"),
            (1e16, "1e+16"),
            (1234567890123456.0, "1234567890123456.0"),
            (-21.123456789, "-21.123456789"),
            (1.5e-7, "1.5e-07"),
            (2.5e100, "2.5e+100"),
            (20037508.342789244, "20037508.342789244"),
        ] {
            assert_eq!(py_float(value), expected, "{value}");
        }
    }

    #[test]
    fn decimal_normalize_matches_python_decimal_str() {
        for (input, expected) in [
            ("1.5", "1.5"),
            ("2.0", "2"),
            ("100.0", "1E+2"),
            ("10", "1E+1"),
            ("5", "5"),
            ("1e-05", "0.00001"),
            ("1e-07", "1E-7"),
            ("-0.0", "-0"),
            ("0.000", "0"),
            ("12.340", "12.34"),
            ("0.5", "0.5"),
        ] {
            assert_eq!(decimal_normalize(input), expected, "{input}");
        }
        assert_eq!(canonical_decimal("-0").as_deref(), Some("0"));
        assert_eq!(canonical_decimal("1E+2"), None);
        assert_eq!(canonical_decimal("0012.50").as_deref(), Some("12.5"));
    }

    #[test]
    fn canonical_json_and_fingerprint_match_python() {
        let mut out = String::new();
        push_canonical(
            &mut out,
            &json!({"b": [1, 2.0, "Đ\n"], "a": null, "c": true}),
        );
        assert_eq!(out, r#"{"a":null,"b":[1,2.0,"\u0110\n"],"c":true}"#);
        // python: hashlib.sha256(json.dumps({"a":1},sort_keys=True,separators=(",",":")).encode()).hexdigest()
        assert_eq!(
            fingerprint(&json!({"a": 1})),
            "015abd7f5cc57a2dd94b7590f04ad8084273905ee33ec5cebeae62276a97f862"
        );
    }

    #[test]
    fn names_are_sanitized_like_python() {
        assert_eq!(
            sanitize_filename("../x/Bản đồ 1.geojson", "f"),
            "B_n_1.geojson"
        );
        assert_eq!(sanitize_filename("...", "fallback"), "fallback");
        let mut used = Vec::new();
        assert_eq!(sanitize_field_id("Mã điểm", &mut used, 0), "M_i_m");
        assert_eq!(sanitize_field_id("1abc", &mut used, 1), "fld_1abc");
        assert_eq!(sanitize_field_id("!!!", &mut used, 2), "fld_2");
        assert_eq!(sanitize_field_id("M ã điểm", &mut used, 3), "M_i_m_2");
    }

    #[test]
    fn parses_geojson_with_order_types_and_findings() {
        let doc = br#"{"type":"FeatureCollection","features":[
            {"type":"Feature","id":"a1","geometry":{"type":"Point","coordinates":[105,21.5]},
             "properties":{"name":"x","n":1,"d":1.5,"t":"2024-01-02T03:04:05+07:00","b":true}},
            {"type":"Feature","geometry":{"type":"Point","coordinates":[106.25,-0.0]},
             "properties":{"name":null,"n":2,"d":2,"t":"2024-01-02T03:04:05Z","b":false}}]}"#;
        let parsed = parse_geojson(doc).unwrap();
        assert!(parsed.findings.is_empty());
        let names: Vec<_> = parsed
            .fields
            .iter()
            .map(|f| (f.field_id.as_str(), f.field_type))
            .collect();
        assert_eq!(
            names,
            [
                ("name", "text"),
                ("n", "integer"),
                ("d", "decimal"),
                ("t", "timestamp"),
                ("b", "boolean")
            ]
        );
        assert!(parsed.fields[0].nullable);
        assert_eq!(parsed.records[0].feature_id, "a1");
        assert_eq!(parsed.records[1].feature_id, "feat-000002");
        assert_eq!(parsed.records[0].attrs[3], json!("2024-01-02T03:04:05Z"));
        assert_eq!(parsed.records[1].attrs[2], json!("2"));
        assert_eq!(parsed.records[1].geom, Geom::Point([106.25, 0.0]));

        let mixed = br#"{"type":"FeatureCollection","features":[
            {"type":"Feature","geometry":{"type":"Point","coordinates":[1,2]},"properties":{}},
            {"type":"Feature","geometry":{"type":"LineString","coordinates":[[1,2],[3,4]]},"properties":null},
            {"type":"Feature","geometry":{"type":"Point","coordinates":[1,2,3]}}]}"#;
        let codes: Vec<_> = parse_geojson(mixed)
            .unwrap()
            .findings
            .iter()
            .map(|f| f.code)
            .collect();
        assert_eq!(codes, ["UNSUPPORTED_DIMENSIONS", "MIXED_GEOMETRY_FAMILY"]);

        for (doc, code) in [
            (
                &b"{\"type\":\"FeatureCollection\",\"crs\":{},\"features\":[]}"[..],
                "LEGACY_GEOJSON_CRS",
            ),
            (&b"{\"type\":\"Feature\"}"[..], "UNSUPPORTED_GEOJSON"),
            (&b"{\"type\":"[..], "INVALID_JSON"),
            (
                &b"{\"type\":\"FeatureCollection\",\"features\":{}}"[..],
                "INVALID_GEOJSON",
            ),
        ] {
            match parse_geojson(doc) {
                Err(TransferError::Coded { code: c, .. }) => assert_eq!(c, code),
                _ => panic!("expected {code}"),
            }
        }
        assert!(matches!(detect_format(b"  {}"), Ok("geojson")));
        assert!(matches!(detect_format(b"PK\x03\x04"), Ok("shapefile_zip")));
        assert!(detect_format(b"hello").is_err());
    }

    #[test]
    fn geojson_writer_matches_json_dumps_layout() {
        let mappings = [Mapping {
            field_id: "name".into(),
            source_name: "Tên".into(),
            output_name: "name".into(),
            field_type: "text".into(),
        }];
        let mut out = String::from(GEOJSON_HEAD);
        push_geojson_feature(
            &mut out,
            "f-1",
            &Geom::Line(vec![[105.0, 21.5], [105.5, 1e-05]]),
            &mappings,
            &json!({"name": "Hà \"Nội\"\n"}),
        )
        .unwrap();
        out.push_str(GEOJSON_TAIL);
        assert_eq!(
            out,
            "{\"type\": \"FeatureCollection\", \"features\": [{\"type\": \"Feature\", \"id\": \"f-1\", \
             \"geometry\": {\"type\": \"LineString\", \"coordinates\": [[105.0, 21.5], [105.5, 1e-05]]}, \
             \"properties\": {\"name\": \"Hà \\\"Nội\\\"\\n\"}}]}\n"
        );
    }

    #[test]
    fn wkb_and_ready_geometry_validation() {
        let mut wkb = vec![1, 3, 0, 0, 0, 1, 0, 0, 0, 4, 0, 0, 0];
        for (x, y) in [(0.0f64, 0.0f64), (1.0, 0.0), (1.0, 1.0), (0.0, 0.0)] {
            wkb.extend_from_slice(&x.to_le_bytes());
            wkb.extend_from_slice(&y.to_le_bytes());
        }
        let geom = decode_wkb(&wkb).unwrap();
        assert_eq!(geom.wkt(), "POLYGON((0.0 0.0, 1.0 0.0, 1.0 1.0, 0.0 0.0))");
        assert!(is_ready(&geom, Some("Polygon"), 4326, false));
        assert!(!is_ready(&geom, Some("Point"), 4326, false));
        assert!(decode_wkb(&wkb[..wkb.len() - 1]).is_none());
        let line = Geom::Line(vec![[179.0, 0.0], [-179.0, 0.0]]);
        assert!(!is_ready(&line, None, 4326, false));
    }

    #[test]
    fn export_plans_and_inspection_fingerprints() {
        let fields = [
            ("long_field_name", "Long", "text"),
            ("price", "Giá", "decimal"),
        ];
        let (m, d, op) = plan_export_mapping(&fields, "shapefile_zip", 4326, 4326, true).unwrap();
        assert_eq!(m[0].output_name, "LONG_FIELD");
        assert_eq!(op, None);
        let codes: Vec<_> = d.iter().map(|c| c.code).collect();
        assert_eq!(
            codes,
            [
                "DBF_FIELD_NAME_TRUNCATED",
                "DBF_DECIMAL_FIXED_PRECISION",
                "DBF_TEXT_TRUNCATED_254"
            ]
        );
        assert!(plan_export_mapping(&fields, "geojson", 4326, 3857, false).is_err());
        let (_, d, op) = plan_export_mapping(&fields, "geojson", 3857, 4326, false).unwrap();
        assert_eq!(op, Some("epsg-1024-inverse"));
        assert_eq!(d[1].code, "GEOJSON_TYPED_STRING_ENCODING");

        let doc = r#"{"type":"FeatureCollection","features":[{"type":"Feature","geometry":{"type":"Point","coordinates":[107.16,20.71]},"properties":{"Mã":1}}]}"#.as_bytes();
        let inspection = build_geojson_inspection(
            doc,
            &InspectOptions {
                asset_id: uuid::Uuid::nil(),
                asset_sha256: "ab",
                selected_member: None,
                declared_source_crs: None,
                target_crs: Some(5899),
                declared_encoding: Some("UTF8"),
            },
        )
        .unwrap();
        assert!(inspection.confirmable);
        assert_eq!(inspection.conversions[0].code, "CRS_TRANSFORMATION");
        assert_eq!(inspection.conversions[1].affected_fields, ["Mã->M"]);
        assert_eq!(inspection.wire["target_crs"], 5899);
    }
}
