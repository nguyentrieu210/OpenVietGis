//! Bounded OGC 3D Tiles 1.1 manifest check, ZIP archive parser, GLB validator, and extrusion generator.

use std::collections::{BTreeMap, BTreeSet, HashSet};

use axum::http::StatusCode;
use ovg_storage::tilesets::TriangulatedRow;
use serde_json::{json, Value};

pub(crate) const MIB: usize = 1024 * 1024;
pub(crate) const MAX_ARCHIVE_BYTES: usize = 64 * MIB;
pub(crate) const MAX_EXTRACTED_BYTES: usize = 128 * MIB;
pub(crate) const MAX_GLB_BYTES: usize = 10 * MIB;
pub(crate) const MAX_FEATURES: usize = 3000;
pub(crate) const MAX_VERTICES: usize = 60_000;
pub(crate) const MAX_TILE_TRIANGLES: usize = 30_000;
pub(crate) const MAX_TILES: usize = 128;

const SEMI_MAJOR_A: f64 = 6_378_137.0;
const E2: f64 = 6.694_379_990_141_316_5e-3;
const DEG_TO_RAD: f64 = std::f64::consts::PI / 180.0;
const MAX_LAT_RAD: f64 = 85.0 * DEG_TO_RAD;
const IDENTITY_MATRIX: [f64; 16] = [
    1.0, 0.0, 0.0, 0.0, //
    0.0, 1.0, 0.0, 0.0, //
    0.0, 0.0, 1.0, 0.0, //
    0.0, 0.0, 0.0, 1.0, //
];

#[derive(Debug)]
pub(crate) struct CodecError {
    pub(crate) status: StatusCode,
    pub(crate) detail: &'static str,
}

impl CodecError {
    pub(crate) fn new(status: StatusCode, detail: &'static str) -> Self {
        Self { status, detail }
    }
}

fn require(ok: bool, detail: &'static str) -> Result<(), CodecError> {
    if ok {
        Ok(())
    } else {
        Err(CodecError::new(StatusCode::UNPROCESSABLE_ENTITY, detail))
    }
}

pub(crate) fn is_valid_tile_uri(uri: &str) -> bool {
    if uri.is_empty() || uri.len() > 180 || !uri.ends_with(".glb") {
        return false;
    }
    let mut segments = uri.split('/');
    let Some(last) = segments.next_back() else {
        return false;
    };
    let Some(stem) = last.strip_suffix(".glb") else {
        return false;
    };
    if stem.is_empty() || !stem.bytes().all(is_uri_char) {
        return false;
    }
    segments.all(|seg| !seg.is_empty() && seg.bytes().all(is_uri_char))
}

fn is_uri_char(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_' || b == b'-'
}

pub(crate) fn is_valid_feature_id(fid: &str) -> bool {
    (1..=128).contains(&fid.len()) && fid.bytes().all(is_uri_char)
}

fn as_num_in(val: Option<&Value>, low: f64, high: f64) -> Option<f64> {
    let v = val?.as_f64()?;
    (v.is_finite() && low <= v && v <= high).then_some(v)
}

fn walk_manifest(
    node: &Value,
    parent_error: f64,
    parent_region: Option<[f64; 6]>,
    parent_matrix: [f64; 16],
    depth: usize,
    count: &mut usize,
    seen: &mut BTreeSet<String>,
) -> Result<(), CodecError> {
    *count += 1;
    let Some(obj) = node.as_object() else {
        return Err(CodecError::new(
            StatusCode::UNPROCESSABLE_ENTITY,
            "Unsupported tileset structure",
        ));
    };
    let allowed = [
        "boundingVolume",
        "geometricError",
        "refine",
        "transform",
        "content",
        "children",
    ];
    require(
        *count <= 256 && depth <= 8 && obj.keys().all(|k| allowed.contains(&k.as_str())),
        "Unsupported tileset structure",
    )?;

    if let Some(refine) = obj.get("refine") {
        require(refine.as_str() == Some("REPLACE"), "Unsupported tileset structure")?;
    }
    let error = as_num_in(obj.get("geometricError"), 0.0, parent_error)
        .ok_or_else(|| CodecError::new(StatusCode::UNPROCESSABLE_ENTITY, "Unsupported tileset structure"))?;

    let bv = obj
        .get("boundingVolume")
        .and_then(Value::as_object)
        .ok_or_else(|| CodecError::new(StatusCode::UNPROCESSABLE_ENTITY, "Unsupported tileset structure"))?;
    require(bv.len() == 1 && bv.contains_key("region"), "Unsupported tileset structure")?;

    let r_vals = bv
        .get("region")
        .and_then(Value::as_array)
        .ok_or_else(|| CodecError::new(StatusCode::UNPROCESSABLE_ENTITY, "Unsupported tileset structure"))?;
    require(r_vals.len() == 6, "Unsupported tileset structure")?;
    let mut r = [0.0_f64; 6];
    for (idx, item) in r_vals.iter().enumerate() {
        r[idx] = as_num_in(Some(item), -12_000.0, 100_000.0).ok_or_else(|| {
            CodecError::new(StatusCode::UNPROCESSABLE_ENTITY, "Unsupported tileset structure")
        })?;
    }
    let [w, s, x, n, h, h_max] = r;
    require(
        -std::f64::consts::PI <= w
            && w <= x
            && x <= std::f64::consts::PI
            && x - w <= std::f64::consts::PI
            && -MAX_LAT_RAD <= s
            && s <= n
            && n <= MAX_LAT_RAD
            && -12_000.0 <= h
            && h <= h_max
            && h_max <= 100_000.0,
        "Unsupported tileset structure",
    )?;
    if let Some(pr) = parent_region {
        require(
            r[0] >= pr[0]
                && r[1] >= pr[1]
                && r[4] >= pr[4]
                && r[2] <= pr[2]
                && r[3] <= pr[3]
                && r[5] <= pr[5],
            "Unsupported tileset structure",
        )?;
    }

    let local = if let Some(t_val) = obj.get("transform") {
        let t_arr = t_val.as_array().ok_or_else(|| {
            CodecError::new(StatusCode::UNPROCESSABLE_ENTITY, "Unsupported tileset structure")
        })?;
        require(t_arr.len() == 16, "Unsupported tileset structure")?;
        let mut t = [0.0_f64; 16];
        for (idx, item) in t_arr.iter().enumerate() {
            t[idx] = as_num_in(Some(item), -1e7, 1e7).ok_or_else(|| {
                CodecError::new(StatusCode::UNPROCESSABLE_ENTITY, "Unsupported tileset structure")
            })?;
        }
        require(
            t[3] == 0.0 && t[7] == 0.0 && t[11] == 0.0 && t[15] == 1.0,
            "Unsupported tileset structure",
        )?;
        t
    } else {
        IDENTITY_MATRIX
    };

    let mut matrix = [0.0_f64; 16];
    for col in 0..4 {
        for row in 0..4 {
            let mut sum = 0.0_f64;
            for k in 0..4 {
                sum += parent_matrix[k * 4 + row] * local[col * 4 + k];
            }
            require(sum.is_finite() && (-1e9..=1e9).contains(&sum), "Unsupported tileset structure")?;
            matrix[col * 4 + row] = sum;
        }
    }

    let has_content = if let Some(c_val) = obj.get("content") {
        let c_obj = c_val.as_object().ok_or_else(|| {
            CodecError::new(StatusCode::UNPROCESSABLE_ENTITY, "Unsupported tileset structure")
        })?;
        require(c_obj.len() == 1, "Unsupported tileset structure")?;
        let uri = c_obj
            .get("uri")
            .and_then(Value::as_str)
            .ok_or_else(|| CodecError::new(StatusCode::UNPROCESSABLE_ENTITY, "Unsupported tileset structure"))?;
        require(
            is_valid_tile_uri(uri) && !seen.contains(uri),
            "Unsupported tileset structure",
        )?;
        seen.insert(uri.to_owned());
        true
    } else {
        false
    };

    let children = match obj.get("children") {
        Some(v) => v.as_array().ok_or_else(|| {
            CodecError::new(StatusCode::UNPROCESSABLE_ENTITY, "Unsupported tileset structure")
        })?,
        None => &Vec::new(),
    };
    require(
        !children.is_empty() || (has_content && error == 0.0),
        "Unsupported tileset structure",
    )?;
    for child in children {
        walk_manifest(child, error, Some(r), matrix, depth + 1, count, seen)?;
    }
    Ok(())
}

pub(crate) fn manifest_check(doc: &Value) -> Result<BTreeSet<String>, CodecError> {
    let Some(obj) = doc.as_object() else {
        return Err(CodecError::new(
            StatusCode::UNPROCESSABLE_ENTITY,
            "Unsupported tileset structure",
        ));
    };
    require(
        obj.keys()
            .all(|k| matches!(k.as_str(), "asset" | "geometricError" | "root")),
        "Unsupported tileset structure",
    )?;
    let asset = obj
        .get("asset")
        .and_then(Value::as_object)
        .ok_or_else(|| CodecError::new(StatusCode::UNPROCESSABLE_ENTITY, "Unsupported tileset structure"))?;
    require(
        asset.get("version").and_then(Value::as_str) == Some("1.1")
            && asset
                .keys()
                .all(|k| matches!(k.as_str(), "version" | "tilesetVersion")),
        "Unsupported tileset structure",
    )?;
    if let Some(tv) = asset.get("tilesetVersion") {
        let s = tv.as_str().ok_or_else(|| {
            CodecError::new(StatusCode::UNPROCESSABLE_ENTITY, "Unsupported tileset structure")
        })?;
        require(
            s.len() <= 160 && s.bytes().all(|b| (b' '..=b'~').contains(&b)),
            "Unsupported tileset structure",
        )?;
    }
    let root_geo_error = as_num_in(obj.get("geometricError"), 0.0, 1e7)
        .ok_or_else(|| CodecError::new(StatusCode::UNPROCESSABLE_ENTITY, "Unsupported tileset structure"))?;
    let root = obj
        .get("root")
        .ok_or_else(|| CodecError::new(StatusCode::UNPROCESSABLE_ENTITY, "Unsupported tileset structure"))?;
    require(
        root.as_object()
            .and_then(|r| r.get("refine"))
            .and_then(Value::as_str)
            == Some("REPLACE"),
        "Unsupported tileset structure",
    )?;
    let mut count = 0usize;
    let mut seen = BTreeSet::new();
    walk_manifest(
        root,
        root_geo_error,
        None,
        IDENTITY_MATRIX,
        0,
        &mut count,
        &mut seen,
    )?;
    require(
        !seen.is_empty() && seen.len() <= MAX_TILES,
        "Unsupported tileset structure",
    )?;
    Ok(seen)
}

fn read_u16_le(bytes: &[u8], off: usize) -> Option<u16> {
    let slice = bytes.get(off..off + 2)?;
    Some(u16::from_le_bytes([slice[0], slice[1]]))
}

fn read_u32_le(bytes: &[u8], off: usize) -> Option<u32> {
    let slice = bytes.get(off..off + 4)?;
    Some(u32::from_le_bytes([slice[0], slice[1], slice[2], slice[3]]))
}

pub(crate) fn validate_model_asset(filename: &str, payload: &[u8]) -> Result<&'static str, CodecError> {
    if filename.is_empty()
        || filename.chars().count() > 180
        || filename
            .bytes()
            .any(|b| b == b'/' || b == b'\\' || b < 0x20 || b == 0x7f)
    {
        return Err(CodecError::new(
            StatusCode::UNPROCESSABLE_ENTITY,
            "Invalid model filename",
        ));
    }
    let Some((_, ext)) = filename.rsplit_once('.') else {
        return Err(CodecError::new(
            StatusCode::UNPROCESSABLE_ENTITY,
            "Only GLB/GLTF supported",
        ));
    };
    let fmt = if ext.eq_ignore_ascii_case("glb") {
        "glb"
    } else if ext.eq_ignore_ascii_case("gltf") {
        "gltf"
    } else {
        return Err(CodecError::new(
            StatusCode::UNPROCESSABLE_ENTITY,
            "Only GLB/GLTF supported",
        ));
    };
    if payload.is_empty() || payload.len() > MAX_GLB_BYTES {
        return Err(CodecError::new(
            StatusCode::PAYLOAD_TOO_LARGE,
            "Model size limit",
        ));
    }
    let ok = if fmt == "glb" {
        validate_glb_inner(payload).is_some()
    } else {
        validate_gltf_inner(payload).is_some()
    };
    if !ok {
        return Err(CodecError::new(
            StatusCode::UNPROCESSABLE_ENTITY,
            "Invalid or unsupported self-contained static model",
        ));
    }
    Ok(fmt)
}

pub(crate) fn validate_glb_bytes(filename: &str, payload: &[u8]) -> Result<(), CodecError> {
    if validate_model_asset(filename, payload)? != "glb" {
        return Err(CodecError::new(
            StatusCode::UNPROCESSABLE_ENTITY,
            "Only GLB/GLTF supported",
        ));
    }
    Ok(())
}

fn validate_model_json(data: &Value, bin_len: usize, allow_uri: bool) -> Option<()> {
    let obj = data.as_object()?;
    if obj.get("asset")?.get("version")?.as_str()? != "2.0" {
        return None;
    }
    let buffers = obj.get("buffers")?.as_array()?;
    if buffers.len() != 1 {
        return None;
    }
    let buf = buffers[0].as_object()?;
    let used_val = buf.get("byteLength")?;
    let used = used_val.as_u64()? as usize;
    if used_val.is_f64() || used == 0 || used > bin_len || bin_len - used > 3 {
        return None;
    }
    if !allow_uri && buf.contains_key("uri") {
        return None;
    }
    for forbidden in [
        "extensionsUsed",
        "extensionsRequired",
        "images",
        "textures",
        "skins",
        "animations",
        "cameras",
    ] {
        if let Some(v) = obj.get(forbidden) {
            let non_empty = match v {
                Value::Null => false,
                Value::Bool(b) => *b,
                Value::Array(a) => !a.is_empty(),
                Value::Object(m) => !m.is_empty(),
                Value::String(s) => !s.is_empty(),
                Value::Number(n) => n.as_f64() != Some(0.0),
            };
            if non_empty {
                return None;
            }
        }
    }
    for (key, max_items) in [
        ("nodes", 256usize),
        ("meshes", 256),
        ("bufferViews", 1024),
        ("accessors", 1024),
        ("scenes", 256),
    ] {
        let arr = obj.get(key)?.as_array()?;
        if arr.is_empty() || arr.len() > max_items {
            return None;
        }
    }
    Some(())
}

fn validate_gltf_inner(payload: &[u8]) -> Option<()> {
    use base64::Engine;
    let data: Value = serde_json::from_slice(payload).ok()?;
    let uri = data.get("buffers")?.as_array()?.first()?.get("uri")?.as_str()?;
    let encoded = uri
        .strip_prefix("data:application/octet-stream;base64,")
        .or_else(|| uri.strip_prefix("data:application/gltf-buffer;base64,"))?;
    let decoded = base64::engine::general_purpose::STANDARD
        .decode(encoded)
        .ok()?;
    validate_model_json(&data, decoded.len(), true)
}

fn validate_glb_inner(payload: &[u8]) -> Option<()> {
    let magic = read_u32_le(payload, 0)?;
    let version = read_u32_le(payload, 4)?;
    let size = read_u32_le(payload, 8)? as usize;
    let json_len = read_u32_le(payload, 12)? as usize;
    let json_kind = read_u32_le(payload, 16)?;
    if (magic, version, size, json_kind) != (0x4654_6C67, 2, payload.len(), 0x4E4F_534A)
        || json_len % 4 != 0
    {
        return None;
    }
    let end = 20_usize.checked_add(json_len)?;
    let json_slice = payload.get(20..end)?;
    let data: Value = serde_json::from_slice(json_slice).ok()?;
    let bin_len = read_u32_le(payload, end)? as usize;
    let bin_kind = read_u32_le(payload, end.checked_add(4)?)?;
    if bin_kind != 0x004E_4942
        || bin_len % 4 != 0
        || end.checked_add(8)?.checked_add(bin_len)? != payload.len()
    {
        return None;
    }
    validate_model_json(&data, bin_len, false)
}

struct ZipEntry<'a> {
    name: &'a str,
    method: u16,
    crc32: u32,
    compressed_size: usize,
    uncompressed_size: usize,
    local_offset: usize,
}

fn find_eocd(payload: &[u8]) -> Option<usize> {
    if payload.len() < 22 {
        return None;
    }
    let min_off = payload.len().saturating_sub(22 + 65_535);
    let max_off = payload.len() - 22;
    for off in (min_off..=max_off).rev() {
        if read_u32_le(payload, off) == Some(0x0605_4B50) {
            let comment_len = read_u16_le(payload, off + 20)? as usize;
            if off + 22 + comment_len == payload.len() {
                return Some(off);
            }
        }
    }
    None
}

fn extract_entry(payload: &[u8], entry: &ZipEntry<'_>) -> Result<Vec<u8>, CodecError> {
    let invalid = || CodecError::new(StatusCode::UNPROCESSABLE_ENTITY, "Invalid ZIP or tileset");
    let off = entry.local_offset;
    if read_u32_le(payload, off) != Some(0x0403_4B50) {
        return Err(invalid());
    }
    let name_len = read_u16_le(payload, off + 26).ok_or_else(invalid)? as usize;
    let extra_len = read_u16_le(payload, off + 28).ok_or_else(invalid)? as usize;
    let local_name = std::str::from_utf8(
        payload
            .get(off + 30..off + 30 + name_len)
            .ok_or_else(invalid)?,
    )
    .map_err(|_| invalid())?;
    if local_name != entry.name {
        return Err(invalid());
    }
    let data_start = off
        .checked_add(30)
        .and_then(|v| v.checked_add(name_len))
        .and_then(|v| v.checked_add(extra_len))
        .ok_or_else(invalid)?;
    let data_end = data_start
        .checked_add(entry.compressed_size)
        .ok_or_else(invalid)?;
    let raw = payload.get(data_start..data_end).ok_or_else(invalid)?;

    let out = match entry.method {
        0 => {
            if raw.len() != entry.uncompressed_size {
                return Err(invalid());
            }
            raw.to_vec()
        }
        8 => miniz_oxide::inflate::decompress_to_vec_with_limit(raw, entry.uncompressed_size)
            .map_err(|_| invalid())?,
        _ => return Err(invalid()),
    };
    if out.len() != entry.uncompressed_size || crc32fast::hash(&out) != entry.crc32 {
        return Err(invalid());
    }
    Ok(out)
}

pub(crate) fn read_archive(
    payload: &[u8],
) -> Result<(Value, BTreeMap<String, Vec<u8>>), CodecError> {
    if payload.len() > MAX_ARCHIVE_BYTES {
        return Err(CodecError::new(
            StatusCode::PAYLOAD_TOO_LARGE,
            "ZIP exceeds 64 MiB",
        ));
    }
    let invalid = || CodecError::new(StatusCode::UNPROCESSABLE_ENTITY, "Invalid ZIP or tileset");
    let eocd = find_eocd(payload).ok_or_else(invalid)?;
    let disk = read_u16_le(payload, eocd + 4).ok_or_else(invalid)?;
    let cd_disk = read_u16_le(payload, eocd + 6).ok_or_else(invalid)?;
    let entries_disk = read_u16_le(payload, eocd + 8).ok_or_else(invalid)?;
    let entries_total = read_u16_le(payload, eocd + 10).ok_or_else(invalid)?;
    let cd_size = read_u32_le(payload, eocd + 12).ok_or_else(invalid)? as usize;
    let cd_offset = read_u32_le(payload, eocd + 16).ok_or_else(invalid)? as usize;
    if disk != 0
        || cd_disk != 0
        || entries_disk != entries_total
        || cd_offset.checked_add(cd_size).is_none_or(|end| end > eocd)
    {
        return Err(invalid());
    }

    let entry_count = entries_total as usize;
    require((2..=129).contains(&entry_count), "Invalid ZIP or tileset")?;

    let mut pos = cd_offset;
    let cd_end = cd_offset + cd_size;
    let mut entries = Vec::with_capacity(entry_count);
    let mut names = BTreeSet::new();
    let mut extracted_total: u64 = 0;

    for _ in 0..entry_count {
        if pos.checked_add(46).is_none_or(|end| end > cd_end)
            || read_u32_le(payload, pos) != Some(0x0201_4B50)
        {
            return Err(invalid());
        }
        let flags = read_u16_le(payload, pos + 8).ok_or_else(invalid)?;
        let method = read_u16_le(payload, pos + 10).ok_or_else(invalid)?;
        let crc32 = read_u32_le(payload, pos + 16).ok_or_else(invalid)?;
        let compressed_u32 = read_u32_le(payload, pos + 20).ok_or_else(invalid)?;
        let uncompressed_u32 = read_u32_le(payload, pos + 24).ok_or_else(invalid)?;
        if compressed_u32 == u32::MAX || uncompressed_u32 == u32::MAX {
            return Err(invalid());
        }
        let name_len = read_u16_le(payload, pos + 28).ok_or_else(invalid)? as usize;
        let extra_len = read_u16_le(payload, pos + 30).ok_or_else(invalid)? as usize;
        let comment_len = read_u16_le(payload, pos + 32).ok_or_else(invalid)? as usize;
        let external_attr = read_u32_le(payload, pos + 38).ok_or_else(invalid)?;
        let local_offset = read_u32_le(payload, pos + 42).ok_or_else(invalid)? as usize;

        let name_start = pos + 46;
        let next_pos = name_start
            .checked_add(name_len)
            .and_then(|v| v.checked_add(extra_len))
            .and_then(|v| v.checked_add(comment_len))
            .ok_or_else(invalid)?;
        if next_pos > cd_end {
            return Err(invalid());
        }
        let name = std::str::from_utf8(&payload[name_start..name_start + name_len])
            .map_err(|_| invalid())?;
        let is_dir = name.ends_with('/') || (external_attr & 0x10) != 0;
        let is_symlink = ((external_attr >> 16) & 0o17_0000) == 0o12_0000;
        if is_dir
            || is_symlink
            || (flags & 1) != 0
            || !matches!(method, 0 | 8)
            || (name != "tileset.json" && !is_valid_tile_uri(name))
            || !names.insert(name.to_owned())
        {
            return Err(invalid());
        }

        let uncompressed_size = uncompressed_u32 as usize;
        extracted_total += uncompressed_size as u64;
        entries.push(ZipEntry {
            name,
            method,
            crc32,
            compressed_size: compressed_u32 as usize,
            uncompressed_size,
            local_offset,
        });
        pos = next_pos;
    }
    if pos != cd_end || !names.contains("tileset.json") {
        return Err(invalid());
    }
    if extracted_total > MAX_EXTRACTED_BYTES as u64 {
        return Err(CodecError::new(
            StatusCode::PAYLOAD_TOO_LARGE,
            "Extracted ZIP exceeds 128 MiB",
        ));
    }
    for e in &entries {
        let limit = if e.name == "tileset.json" {
            MIB
        } else {
            MAX_GLB_BYTES
        };
        if e.uncompressed_size > limit {
            return Err(CodecError::new(
                StatusCode::PAYLOAD_TOO_LARGE,
                "Resource size limit",
            ));
        }
    }

    let manifest_entry = entries
        .iter()
        .find(|e| e.name == "tileset.json")
        .ok_or_else(invalid)?;
    let manifest_bytes = extract_entry(payload, manifest_entry)?;
    let doc: Value = serde_json::from_slice(&manifest_bytes).map_err(|_| invalid())?;
    let uris = manifest_check(&doc)?;

    let mut expected_names = uris.clone();
    expected_names.insert("tileset.json".to_owned());
    require(
        names == expected_names,
        "ZIP must contain exactly referenced resources",
    )?;

    let mut resources = BTreeMap::new();
    for uri in uris {
        let entry = entries
            .iter()
            .find(|e| e.name == uri)
            .ok_or_else(invalid)?;
        let data = extract_entry(payload, entry)?;
        let filename = uri.rsplit('/').next().unwrap_or(&uri);
        validate_glb_bytes(filename, &data)?;
        resources.insert(uri, data);
    }

    Ok((doc, resources))
}

fn ecef(lon_deg: f64, lat_deg: f64, height: f64) -> [f64; 3] {
    let lon = lon_deg * DEG_TO_RAD;
    let lat = lat_deg * DEG_TO_RAD;
    let sin_lat = lat.sin();
    let cos_lat = lat.cos();
    let n = SEMI_MAJOR_A / (1.0 - E2 * sin_lat * sin_lat).sqrt();
    [
        (n + height) * cos_lat * lon.cos(),
        (n + height) * cos_lat * lon.sin(),
        (n * (1.0 - E2) + height) * sin_lat,
    ]
}

fn build_frame(lon_deg: f64, lat_deg: f64, height: f64) -> ([f64; 16], [f64; 3], [f64; 3], [f64; 3], [f64; 3]) {
    let lam = lon_deg * DEG_TO_RAD;
    let phi = lat_deg * DEG_TO_RAD;
    let east = [-lam.sin(), lam.cos(), 0.0];
    let north = [
        -phi.sin() * lam.cos(),
        -phi.sin() * lam.sin(),
        phi.cos(),
    ];
    let up = [
        phi.cos() * lam.cos(),
        phi.cos() * lam.sin(),
        phi.sin(),
    ];
    let origin = ecef(lon_deg, lat_deg, height);
    let matrix = [
        east[0], east[1], east[2], 0.0, //
        north[0], north[1], north[2], 0.0, //
        up[0], up[1], up[2], 0.0, //
        origin[0], origin[1], origin[2], 1.0, //
    ];
    (matrix, east, north, up, origin)
}

fn to_local(
    point: [f64; 3],
    east: [f64; 3],
    north: [f64; 3],
    up: [f64; 3],
    origin: [f64; 3],
) -> Result<[f32; 3], CodecError> {
    let [x, y, z] = ecef(point[0], point[1], point[2]);
    let d = [x - origin[0], y - origin[1], z - origin[2]];
    let e = d[0] * east[0] + d[1] * east[1] + d[2] * east[2];
    let n = d[0] * north[0] + d[1] * north[1] + d[2] * north[2];
    let u = d[0] * up[0] + d[1] * up[1] + d[2] * up[2];
    require(
        e.abs().max(n.abs()).max(u.abs()) <= 10_000.0,
        "Tile local coordinates exceed 10 km",
    )?;
    Ok([e as f32, u as f32, (-n) as f32])
}

fn orient_ring(ring: &[[f64; 2]], ccw: bool) -> Result<Vec<[f64; 2]>, CodecError> {
    if ring.is_empty() || ring[0] != ring[ring.len() - 1] {
        return Err(CodecError::new(
            StatusCode::UNPROCESSABLE_ENTITY,
            "Degenerate polygon ring",
        ));
    }
    let [ox, oy] = ring[0];
    let area: f64 = ring
        .windows(2)
        .map(|w| (w[0][0] - ox) * (w[1][1] - oy) - (w[1][0] - ox) * (w[0][1] - oy))
        .sum();
    require(area != 0.0, "Degenerate polygon ring")?;
    if (area > 0.0) == ccw {
        Ok(ring.to_vec())
    } else {
        Ok(ring.iter().copied().rev().collect())
    }
}

fn parse_decimal_height(s: &str) -> Result<f64, CodecError> {
    let (int_part, frac_part) = match s.split_once('.') {
        Some((i, f)) => (i, f),
        None => (s, ""),
    };
    let valid_syntax = (1..=4).contains(&int_part.len())
        && int_part.bytes().all(|b| b.is_ascii_digit())
        && (s.split_once('.').is_none()
            || ((1..=18).contains(&frac_part.len()) && frac_part.bytes().all(|b| b.is_ascii_digit())));
    require(valid_syntax, "Invalid exact decimal height")?;
    let int_val: u64 = int_part.parse().map_err(|_| {
        CodecError::new(StatusCode::UNPROCESSABLE_ENTITY, "Invalid exact decimal height")
    })?;
    let mut frac_padded = String::with_capacity(18);
    frac_padded.push_str(frac_part);
    while frac_padded.len() < 18 {
        frac_padded.push('0');
    }
    let frac_val: u64 = frac_padded.parse().map_err(|_| {
        CodecError::new(StatusCode::UNPROCESSABLE_ENTITY, "Invalid exact decimal height")
    })?;
    let in_range = (int_val > 0 || frac_val >= 10_000_000_000_000_000)
        && (int_val < 3000 || (int_val == 3000 && frac_val == 0));
    require(in_range, "Height outside [0.01,3000] m")?;
    s.parse::<f64>().map_err(|_| {
        CodecError::new(StatusCode::UNPROCESSABLE_ENTITY, "Invalid exact decimal height")
    })
}

fn parse_ring_points(val: &Value) -> Result<Vec<[f64; 2]>, CodecError> {
    let arr = val.as_array().ok_or_else(|| {
        CodecError::new(StatusCode::UNPROCESSABLE_ENTITY, "Unsupported geographic extent")
    })?;
    let mut out = Vec::with_capacity(arr.len());
    for pt in arr {
        let coords = pt.as_array().ok_or_else(|| {
            CodecError::new(StatusCode::UNPROCESSABLE_ENTITY, "Unsupported geographic extent")
        })?;
        require(coords.len() == 2, "Unsupported geographic extent")?;
        let lon = coords[0].as_f64().ok_or_else(|| {
            CodecError::new(StatusCode::UNPROCESSABLE_ENTITY, "Unsupported geographic extent")
        })?;
        let lat = coords[1].as_f64().ok_or_else(|| {
            CodecError::new(StatusCode::UNPROCESSABLE_ENTITY, "Unsupported geographic extent")
        })?;
        require(
            lon.is_finite()
                && lat.is_finite()
                && (-179.99..=179.99).contains(&lon)
                && (-84.9..=84.9).contains(&lat),
            "Unsupported geographic extent",
        )?;
        out.push([lon, lat]);
    }
    Ok(out)
}

fn feature_mesh(row: &TriangulatedRow, base: f64) -> Result<(Vec<[f64; 3]>, [f64; 6]), CodecError> {
    let height = match &row.height {
        Value::String(s) => parse_decimal_height(s)?,
        Value::Number(n) => {
            let v = n.as_f64().unwrap_or(f64::NAN);
            require(
                v.is_finite() && (0.01..=3000.0).contains(&v),
                "Every feature needs a metre height in [0.01,3000]",
            )?;
            v
        }
        _ => {
            return Err(CodecError::new(
                StatusCode::UNPROCESSABLE_ENTITY,
                "Every feature needs a metre height in [0.01,3000]",
            ))
        }
    };
    require(
        height.is_finite() && (0.01..=3000.0).contains(&height),
        "Every feature needs a metre height in [0.01,3000]",
    )?;

    require(
        row.geometry.get("type").and_then(Value::as_str) == Some("Polygon")
            && row.triangles.get("type").and_then(Value::as_str) == Some("GeometryCollection"),
        "Expected triangulated polygon",
    )?;
    let raw_rings = row
        .geometry
        .get("coordinates")
        .and_then(Value::as_array)
        .ok_or_else(|| CodecError::new(StatusCode::UNPROCESSABLE_ENTITY, "Empty polygon"))?;
    require(!raw_rings.is_empty(), "Empty polygon")?;

    let mut rings = Vec::with_capacity(raw_rings.len());
    let mut west = f64::INFINITY;
    let mut south = f64::INFINITY;
    let mut east = f64::NEG_INFINITY;
    let mut north = f64::NEG_INFINITY;
    for raw_ring in raw_rings {
        let pts = parse_ring_points(raw_ring)?;
        for [lon, lat] in &pts {
            west = west.min(*lon);
            east = east.max(*lon);
            south = south.min(*lat);
            north = north.max(*lat);
        }
        rings.push(pts);
    }
    require(
        west.is_finite() && east - west <= 0.02 && north - south <= 0.02,
        "Footprint exceeds 0.02 degree extent",
    )?;

    let top = base + height;
    let tri_geoms = row
        .triangles
        .get("geometries")
        .and_then(Value::as_array)
        .ok_or_else(|| CodecError::new(StatusCode::UNPROCESSABLE_ENTITY, "Invalid triangulation"))?;
    let mut mesh = Vec::new();
    for tri in tri_geoms {
        let tri_coords = tri
            .get("type")
            .and_then(Value::as_str)
            .filter(|t| *t == "Polygon")
            .and_then(|_| tri.get("coordinates").and_then(Value::as_array))
            .ok_or_else(|| {
                CodecError::new(StatusCode::UNPROCESSABLE_ENTITY, "Invalid triangulation")
            })?;
        require(tri_coords.len() == 1, "Invalid triangulation")?;
        let ring = parse_ring_points(&tri_coords[0])?;
        require(ring.len() == 4, "Invalid triangle")?;
        let oriented = orient_ring(&ring, true)?;
        let [a, b, c] = [oriented[0], oriented[1], oriented[2]];
        mesh.extend_from_slice(&[
            [a[0], a[1], top],
            [b[0], b[1], top],
            [c[0], c[1], top],
            [a[0], a[1], base],
            [c[0], c[1], base],
            [b[0], b[1], base],
        ]);
    }
    require(!mesh.is_empty(), "Polygon has no triangles")?;

    for (idx, ring) in rings.into_iter().enumerate() {
        let oriented = orient_ring(&ring, idx == 0)?;
        for w in oriented.windows(2) {
            let a = w[0];
            let b = w[1];
            require(a != b, "Repeated boundary vertex")?;
            mesh.extend_from_slice(&[
                [a[0], a[1], base],
                [b[0], b[1], base],
                [b[0], b[1], top],
                [a[0], a[1], base],
                [b[0], b[1], top],
                [a[0], a[1], top],
            ]);
        }
    }

    Ok((mesh, [west, south, east, north, base, top]))
}

fn union_bounds(bounds: &[[f64; 6]]) -> [f64; 6] {
    let mut out = bounds[0];
    for b in &bounds[1..] {
        out[0] = out[0].min(b[0]);
        out[1] = out[1].min(b[1]);
        out[2] = out[2].max(b[2]);
        out[3] = out[3].max(b[3]);
        out[4] = out[4].min(b[4]);
        out[5] = out[5].max(b[5]);
    }
    out
}

fn region_from_bounds(b: [f64; 6]) -> [f64; 6] {
    [
        b[0] * DEG_TO_RAD - 1e-6,
        b[1] * DEG_TO_RAD - 1e-6,
        b[2] * DEG_TO_RAD + 1e-6,
        b[3] * DEG_TO_RAD + 1e-6,
        b[4] - 5.0,
        b[5] + 5.0,
    ]
}

fn build_glb(
    points: &[[f64; 3]],
    east: [f64; 3],
    north: [f64; 3],
    up: [f64; 3],
    origin: [f64; 3],
) -> Result<Vec<u8>, CodecError> {
    let mut min = [f32::INFINITY; 3];
    let mut max = [f32::NEG_INFINITY; 3];
    let mut binary = Vec::with_capacity(points.len() * 12);
    for p in points {
        let local = to_local(*p, east, north, up, origin)?;
        for axis in 0..3 {
            min[axis] = min[axis].min(local[axis]);
            max[axis] = max[axis].max(local[axis]);
            binary.extend_from_slice(&local[axis].to_le_bytes());
        }
    }
    let doc = json!({
        "asset": { "version": "2.0" },
        "buffers": [{ "byteLength": binary.len() }],
        "bufferViews": [{ "buffer": 0, "byteLength": binary.len(), "target": 34962 }],
        "accessors": [{
            "bufferView": 0,
            "componentType": 5126,
            "count": points.len(),
            "type": "VEC3",
            "min": [f64::from(min[0]), f64::from(min[1]), f64::from(min[2])],
            "max": [f64::from(max[0]), f64::from(max[1]), f64::from(max[2])],
        }],
        "materials": [{
            "pbrMetallicRoughness": {
                "baseColorFactor": [0.24, 0.56, 0.79, 1],
                "metallicFactor": 0,
                "roughnessFactor": 1,
            }
        }],
        "meshes": [{ "primitives": [{ "attributes": { "POSITION": 0 }, "material": 0 }] }],
        "nodes": [{ "mesh": 0 }],
        "scenes": [{ "nodes": [0] }],
        "scene": 0,
    });
    let mut raw = serde_json::to_vec(&doc).map_err(|_| {
        CodecError::new(StatusCode::UNPROCESSABLE_ENTITY, "GLB serialization failed")
    })?;
    while raw.len() % 4 != 0 {
        raw.push(b' ');
    }
    let total_len = 28 + raw.len() + binary.len();
    let mut out = Vec::with_capacity(total_len);
    out.extend_from_slice(&0x4654_6C67_u32.to_le_bytes());
    out.extend_from_slice(&2_u32.to_le_bytes());
    out.extend_from_slice(&(total_len as u32).to_le_bytes());
    out.extend_from_slice(&(raw.len() as u32).to_le_bytes());
    out.extend_from_slice(&0x4E4F_534A_u32.to_le_bytes());
    out.extend_from_slice(&raw);
    out.extend_from_slice(&(binary.len() as u32).to_le_bytes());
    out.extend_from_slice(&0x004E_4942_u32.to_le_bytes());
    out.extend_from_slice(&binary);
    Ok(out)
}

pub(crate) struct GeneratedTileset {
    pub(crate) manifest: Value,
    pub(crate) resources: BTreeMap<String, Vec<u8>>,
    pub(crate) total_triangles: usize,
    pub(crate) indices: BTreeMap<String, Value>,
}

pub(crate) fn build_tiles(rows: &[TriangulatedRow], base: f64) -> Result<GeneratedTileset, CodecError> {
    require(
        (1..=MAX_FEATURES).contains(&rows.len()),
        "Feature count outside generator budget",
    )?;
    require(
        base.is_finite() && (-1000.0..=10_000.0).contains(&base),
        "Unsupported base elevation",
    )?;

    let mut buckets: BTreeMap<(i64, i64), Vec<(Vec<[f64; 3]>, [f64; 6], String)>> = BTreeMap::new();
    let mut total = 0usize;
    let mut vertex_count = 0usize;
    let mut seen = HashSet::with_capacity(rows.len());

    for row in rows {
        require(
            is_valid_feature_id(&row.feature_id) && seen.insert(row.feature_id.clone()),
            "Invalid or duplicate feature ID",
        )?;
        let coords = row
            .geometry
            .get("coordinates")
            .and_then(Value::as_array)
            .ok_or_else(|| CodecError::new(StatusCode::UNPROCESSABLE_ENTITY, "Empty polygon"))?;
        for ring in coords {
            vertex_count += ring.as_array().map_or(0, Vec::len);
        }
        require(vertex_count <= MAX_VERTICES, "Vertex budget exceeded")?;
        let (mesh, bounds) = feature_mesh(row, base)?;
        let tri_count = mesh.len() / 3;
        total += tri_count;
        require(
            tri_count <= MAX_TILE_TRIANGLES,
            "Feature exceeds per-tile triangle budget",
        )?;
        let bx = ((bounds[0] + bounds[2]) / 0.02).floor() as i64;
        let by = ((bounds[1] + bounds[3]) / 0.02).floor() as i64;
        buckets
            .entry((bx, by))
            .or_default()
            .push((mesh, bounds, row.feature_id.clone()));
    }

    let mut resources = BTreeMap::new();
    let mut children = Vec::new();
    let mut child_regions = Vec::new();
    let mut indices = BTreeMap::new();

    let mut emit = |mesh: Vec<[f64; 3]>, bounds: Vec<[f64; 6]>, ranges: Vec<Value>| -> Result<(), CodecError> {
        require(children.len() < MAX_TILES, "Tile count exceeds 128")?;
        let b = union_bounds(&bounds);
        let (matrix, east, north, up, origin) =
            build_frame((b[0] + b[2]) / 2.0, (b[1] + b[3]) / 2.0, base);
        let uri = format!("tiles/tile-{:04}.glb", children.len());
        let glb_bytes = build_glb(&mesh, east, north, up, origin)?;
        let reg = region_from_bounds(b);
        child_regions.push(reg);
        resources.insert(uri.clone(), glb_bytes);
        indices.insert(uri.clone(), Value::Array(ranges));
        children.push(json!({
            "boundingVolume": { "region": reg },
            "geometricError": 0,
            "transform": matrix,
            "content": { "uri": uri },
        }));
        Ok(())
    };

    for (_, features) in buckets {
        let mut mesh = Vec::new();
        let mut bounds = Vec::new();
        let mut ranges = Vec::new();
        for (vertices, b, fid) in features {
            if mesh.len() + vertices.len() > 3 * MAX_TILE_TRIANGLES {
                emit(mesh, bounds, ranges)?;
                mesh = Vec::new();
                bounds = Vec::new();
                ranges = Vec::new();
            }
            ranges.push(json!({
                "feature_id": fid,
                "first_triangle": mesh.len() / 3,
                "triangle_count": vertices.len() / 3,
            }));
            mesh.extend(vertices);
            bounds.push(b);
        }
        emit(mesh, bounds, ranges)?;
    }

    let root_region = union_bounds(&child_regions);
    require(
        root_region[2] - root_region[0] <= std::f64::consts::PI,
        "Tileset spans more than 180 degrees",
    )?;
    let manifest = json!({
        "asset": { "version": "1.1" },
        "geometricError": 1000,
        "root": {
            "boundingVolume": { "region": root_region },
            "geometricError": 1000,
            "refine": "REPLACE",
            "children": children,
        },
    });
    Ok(GeneratedTileset {
        manifest,
        resources,
        total_triangles: total,
        indices,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tile_uri_validation_matches_regex() {
        assert!(is_valid_tile_uri("root.glb"));
        assert!(is_valid_tile_uri("tiles/tile-0001_a.glb"));
        assert!(!is_valid_tile_uri("tiles/../tile.glb"));
        assert!(!is_valid_tile_uri("/root.glb"));
        assert!(!is_valid_tile_uri("tiles/tile.gltf"));
        assert!(!is_valid_tile_uri(""));
    }

    #[test]
    fn decimal_height_bounds_match_python() {
        assert!(parse_decimal_height("0.01").is_ok());
        assert!(parse_decimal_height("3000").is_ok());
        assert!(parse_decimal_height("3000.000").is_ok());
        assert!(parse_decimal_height("0.009999999999999999").is_err());
        assert!(parse_decimal_height("3000.000000000000000001").is_err());
        assert!(parse_decimal_height("-1").is_err());
    }

    #[test]
    fn generated_glb_passes_validator_and_manifest_check() {
        let rows = vec![TriangulatedRow {
            feature_id: "bldg-1".to_owned(),
            height: json!("12.50"),
            geometry: json!({
                "type": "Polygon",
                "coordinates": [[[106.7, 10.77], [106.7001, 10.77], [106.7001, 10.7701], [106.7, 10.7701], [106.7, 10.77]]],
            }),
            triangles: json!({
                "type": "GeometryCollection",
                "geometries": [
                    {"type": "Polygon", "coordinates": [[[106.7, 10.77], [106.7001, 10.77], [106.7001, 10.7701], [106.7, 10.77]]]},
                    {"type": "Polygon", "coordinates": [[[106.7, 10.77], [106.7001, 10.7701], [106.7, 10.7701], [106.7, 10.77]]]},
                ],
            }),
        }];
        let out = build_tiles(&rows, 5.0).unwrap();
        let uris = manifest_check(&out.manifest).unwrap();
        assert_eq!(uris.len(), 1);
        for (uri, payload) in &out.resources {
            validate_glb_bytes(uri.rsplit('/').next().unwrap(), payload).unwrap();
        }
    }
}
