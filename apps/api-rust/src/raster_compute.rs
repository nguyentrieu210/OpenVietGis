//! E09 compute control plane. GDAL/PROJ/NumPy remain bounded format engines,
//! while Rust owns authentication, validation, quota, artifacts and transactions.

use axum::body::to_bytes;
use axum::extract::{Path, Request, State};
use axum::http::{header, HeaderMap, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::post;
use axum::{Json, Router};
use base64::{engine::general_purpose::STANDARD, Engine};
use ovg_core::AppError;
use ovg_storage::catalog::{ProjectRead, ProjectReadPolicy};
use ovg_storage::raster::{self, NewRaster, RasterTile};
use serde::Deserialize;
use serde_json::{json, Value};
use std::borrow::Cow;
use std::collections::{HashMap, HashSet};
use std::path::{Path as FsPath, PathBuf};
use std::process::Stdio;
use std::time::Duration;
use tokio::io::AsyncReadExt;
use tokio::sync::{Semaphore, SemaphorePermit};
use uuid::Uuid;

use crate::catalog::{project_read, CatalogError};
use crate::identity::{require_csrf, HumanSession};
use crate::raster_policy::{valid_geotiff_magic, validate_store_intent, RasterMethod, StoreIntent};
use crate::transfers::store::multipart_parts;
use crate::AppState;

const BASE: &str = "/organizations/{organization_id}/projects/{project_id}/raster";
const MIB: usize = 1024 * 1024;
const MAX_INPUT: usize = 64 * MIB;
const MAX_COMBINED: usize = 96 * MIB;
const MAX_OUTPUT: usize = 128 * MIB;
const WORKER: &str = include_str!("../../api/openvietgis_api/adapters/formats/raster_worker.py");
const PERSISTENT: &str =
    include_str!("../../api/openvietgis_api/adapters/formats/persistent_raster_worker.py");
const GEOREF: &str =
    include_str!("../../api/openvietgis_api/adapters/formats/georeference_worker.py");
const TERRAIN: &str = include_str!("../../api/openvietgis_api/adapters/formats/terrain_worker.py");
static GATE: Semaphore = Semaphore::const_new(1);

pub(crate) fn router() -> Router<AppState> {
    Router::new()
        .route(&format!("{BASE}/analyze"), post(analyze))
        .route(&format!("{BASE}/georeference"), post(georeference))
        .route(&format!("{BASE}/layers"), post(store))
        .route(&format!("{BASE}/reports"), post(report))
        .route(
            &format!("{BASE}/layers/{{layer_id}}/terrain-preview"),
            post(terrain),
        )
}

enum ComputeError {
    Access(CatalogError),
    Domain(AppError),
    Detail(StatusCode, String),
    InvalidRequest,
}

impl From<CatalogError> for ComputeError {
    fn from(error: CatalogError) -> Self {
        Self::Access(error)
    }
}

impl From<AppError> for ComputeError {
    fn from(error: AppError) -> Self {
        Self::Domain(error)
    }
}

impl IntoResponse for ComputeError {
    fn into_response(self) -> Response {
        match self {
            Self::Access(error) => error.into_response(),
            Self::Domain(error) => CatalogError::Application(error).into_response(),
            Self::Detail(status, detail) => {
                (status, Json(json!({"detail": detail}))).into_response()
            }
            Self::InvalidRequest => CatalogError::InvalidRequest.into_response(),
        }
    }
}

type Result<T> = std::result::Result<T, ComputeError>;

fn detail(status: StatusCode, message: impl Into<String>) -> ComputeError {
    ComputeError::Detail(status, message.into())
}

fn invalid(message: impl Into<String>) -> ComputeError {
    detail(StatusCode::UNPROCESSABLE_ENTITY, message)
}

fn too_large(message: &str) -> ComputeError {
    detail(StatusCode::PAYLOAD_TOO_LARGE, message)
}

fn gate() -> Result<SemaphorePermit<'static>> {
    GATE.try_acquire().map_err(|_| {
        detail(
            StatusCode::TOO_MANY_REQUESTS,
            "Raster worker busy; retry shortly",
        )
    })
}

struct Scratch(PathBuf);

impl Scratch {
    fn create() -> Result<Self> {
        let path = std::env::temp_dir().join(format!("ovg-raster-{}", Uuid::new_v4()));
        let mut builder = std::fs::DirBuilder::new();
        #[cfg(unix)]
        {
            use std::os::unix::fs::DirBuilderExt;
            builder.mode(0o700);
        }
        builder
            .create(&path)
            .map_err(|_| invalid("Raster workspace unavailable"))?;
        Ok(Self(path))
    }

    fn path(&self, name: &str) -> PathBuf {
        self.0.join(name)
    }

    async fn stage(&self, name: &str, data: &[u8], limit: usize) -> Result<PathBuf> {
        if data.len() > limit {
            return Err(too_large(&format!(
                "Each GeoTIFF is limited to {} MiB",
                limit / MIB
            )));
        }
        let magic = data.get(..4).and_then(|v| v.try_into().ok());
        if !magic.is_some_and(valid_geotiff_magic) {
            return Err(invalid("Only local GeoTIFF files are accepted"));
        }
        let path = self.path(name);
        tokio::fs::write(&path, data)
            .await
            .map_err(|_| invalid("Raster workspace unavailable"))?;
        Ok(path)
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

pub(crate) fn worker_nonroot() -> bool {
    // Production image runs UID 10001. Fail closed rather than launch a codec
    // as root if an operator overrides that security boundary.
    std::fs::read_to_string("/proc/self/status")
        .ok()
        .and_then(|status| {
            status.lines().find_map(|line| {
                line.strip_prefix("Uid:")
                    .and_then(|uid| uid.split_whitespace().nth(1))
                    .and_then(|uid| uid.parse::<u32>().ok())
            })
        })
        .is_some_and(|uid| uid != 0)
}

async fn worker(
    scratch: &Scratch,
    script: &str,
    args: &[String],
    persistent: bool,
    seconds: u64,
    output_limit: usize,
) -> Result<Value> {
    if !worker_nonroot() {
        return Err(invalid(
            "Raster worker requires an unprivileged Linux runtime",
        ));
    }
    for (name, source) in [
        ("raster_worker.py", WORKER),
        ("persistent_raster_worker.py", PERSISTENT),
        ("terrain_worker.py", TERRAIN),
        ("georeference_worker.py", GEOREF),
    ] {
        tokio::fs::write(scratch.path(name), source)
            .await
            .map_err(|_| invalid("Raster worker unavailable"))?;
    }
    let mut cmd = tokio::process::Command::new("/usr/bin/python3");
    cmd.args(["-E", "-s", "-B"])
        .arg(scratch.path(script))
        .args(args)
        .current_dir(&scratch.0)
        .env_clear()
        .env("PATH", "/usr/bin:/bin")
        .env("HOME", &scratch.0)
        .env("LANG", "C.UTF-8")
        .env("PROJ_NETWORK", "OFF")
        .env("GDAL_NUM_THREADS", "1")
        .env("OPENBLAS_NUM_THREADS", "1")
        .env("GDAL_DISABLE_READDIR_ON_OPEN", "EMPTY_DIR")
        .env("GDAL_PAM_ENABLED", "NO")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true);
    if persistent {
        cmd.env("OVG_RASTER_PERSIST", "1");
    }
    let mut child = cmd
        .spawn()
        .map_err(|_| invalid("Raster worker unavailable or resource limit exceeded"))?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| invalid("Raster worker unavailable"))?;
    let (status, bytes) = tokio::time::timeout(Duration::from_secs(seconds), async {
        let mut bytes = Vec::new();
        stdout
            .take(output_limit as u64 + 1)
            .read_to_end(&mut bytes)
            .await?;
        if bytes.len() > output_limit {
            let _ = child.kill().await;
            return Err(std::io::Error::other("raster output limit"));
        }
        Ok::<_, std::io::Error>((child.wait().await?, bytes))
    })
    .await
    .map_err(|_| invalid("Raster worker unavailable or resource limit exceeded"))?
    .map_err(|_| invalid("Raster worker unavailable or resource limit exceeded"))?;
    let value: Value =
        serde_json::from_slice(&bytes).map_err(|_| invalid("Unable to process this GeoTIFF"))?;
    if !status.success() || value.get("error").is_some() {
        let message = value
            .get("error")
            .and_then(Value::as_str)
            .unwrap_or("Raster processing failed")
            .replace(scratch.0.to_string_lossy().as_ref(), "<raster>");
        return Err(invalid(message.chars().take(300).collect::<String>()));
    }
    if !value.is_object() {
        return Err(invalid("Invalid raster output"));
    }
    Ok(value)
}

async fn read_output(path: &FsPath, limit: usize) -> Result<Vec<u8>> {
    let info = tokio::fs::symlink_metadata(path)
        .await
        .map_err(|_| invalid("Invalid raster output"))?;
    if !info.is_file() || info.len() == 0 || info.len() > limit as u64 {
        return Err(invalid("Invalid raster output size"));
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        if info.nlink() != 1 {
            return Err(invalid("Invalid raster output"));
        }
    }
    let file = tokio::fs::File::open(path)
        .await
        .map_err(|_| invalid("Invalid raster output"))?;
    let mut bytes = Vec::new();
    file.take(limit as u64 + 1)
        .read_to_end(&mut bytes)
        .await
        .map_err(|_| invalid("Invalid raster output"))?;
    if bytes.is_empty() || bytes.len() > limit {
        return Err(invalid("Invalid raster output size"));
    }
    Ok(bytes)
}

struct Form<'a>(HashMap<String, (bool, Cow<'a, [u8]>)>);

impl<'a> Form<'a> {
    fn parse(content_type: &str, bytes: &'a [u8]) -> Result<Self> {
        let mut result = HashMap::new();
        if content_type.split(';').next().is_some_and(|kind| {
            kind.trim()
                .eq_ignore_ascii_case("application/x-www-form-urlencoded")
        }) {
            for (name, value) in url::form_urlencoded::parse(bytes) {
                if result
                    .insert(
                        name.into_owned(),
                        (false, Cow::Owned(value.into_owned().into_bytes())),
                    )
                    .is_some()
                {
                    return Err(ComputeError::InvalidRequest);
                }
            }
        } else {
            for (name, filename, content) in
                multipart_parts(content_type, bytes).ok_or(ComputeError::InvalidRequest)?
            {
                if result
                    .insert(name, (filename.is_some(), Cow::Borrowed(content)))
                    .is_some()
                {
                    return Err(ComputeError::InvalidRequest);
                }
            }
        }
        if result.len() > 24 {
            return Err(ComputeError::InvalidRequest);
        }
        Ok(Self(result))
    }

    fn file(&self, name: &str) -> Result<Option<&[u8]>> {
        match self.0.get(name) {
            Some((true, data)) => Ok(Some(data.as_ref())),
            Some((false, _)) => Err(ComputeError::InvalidRequest),
            None => Ok(None),
        }
    }

    fn text(&self, name: &str) -> Result<Option<&str>> {
        match self.0.get(name) {
            Some((false, data)) => std::str::from_utf8(data)
                .map(Some)
                .map_err(|_| ComputeError::InvalidRequest),
            Some((true, _)) => Err(ComputeError::InvalidRequest),
            None => Ok(None),
        }
    }

    fn required(&self, name: &str) -> Result<&str> {
        self.text(name)?.ok_or(ComputeError::InvalidRequest)
    }

    fn band(&self, name: &str, default: u8) -> Result<u8> {
        let value = match self.text(name)? {
            Some(s) => s.parse::<u8>().map_err(|_| ComputeError::InvalidRequest)?,
            None => default,
        };
        if !(1..=16).contains(&value) {
            return Err(ComputeError::InvalidRequest);
        }
        Ok(value)
    }

    fn id(&self, name: &str) -> Result<Option<Uuid>> {
        self.text(name)?
            .map(|s| Uuid::parse_str(s).map_err(|_| ComputeError::InvalidRequest))
            .transpose()
    }
}

async fn body(req: Request, limit: usize) -> Result<(String, axum::body::Bytes)> {
    let content_type = req
        .headers()
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .ok_or(ComputeError::InvalidRequest)?
        .to_owned();
    let data = to_bytes(req.into_body(), limit)
        .await
        .map_err(|_| too_large("Raster request exceeds input limit"))?;
    Ok((content_type, data))
}

fn parse_method(method: &str) -> Result<RasterMethod> {
    Ok(match method {
        "band" => RasterMethod::Band,
        "ndvi" => RasterMethod::Ndvi,
        "ndwi" => RasterMethod::Ndwi,
        "ndbi" => RasterMethod::Ndbi,
        "hillshade" => RasterMethod::Hillshade,
        "calculator" => RasterMethod::Calculator,
        "rgb" => RasterMethod::Rgb,
        "clip" => RasterMethod::Clip,
        "change" => RasterMethod::Change,
        "slope" => RasterMethod::Slope,
        "aspect" => RasterMethod::Aspect,
        "mosaic" => RasterMethod::Mosaic,
        "resample" => RasterMethod::Resample,
        _ => return Err(ComputeError::InvalidRequest),
    })
}

async fn authorize<'a>(
    state: &'a AppState,
    auth: &HumanSession,
    ids: (Uuid, Uuid),
    write: bool,
) -> Result<ProjectRead<'a>> {
    let policy = if write {
        ProjectReadPolicy::UnrestrictedEdit
    } else {
        ProjectReadPolicy::Unrestricted
    };
    Ok(project_read(state, auth, ids.0, ids.1, policy).await?)
}

async fn analyze(
    State(state): State<AppState>,
    auth: HumanSession,
    Path(ids): Path<(Uuid, Uuid)>,
    req: Request,
) -> Result<Json<Value>> {
    require_csrf(&state, &auth, req.headers()).map_err(CatalogError::Identity)?;
    authorize(&state, &auth, ids, false).await?.commit().await?;
    let _permit = gate()?;
    let (kind, data) = body(req, 65 * MIB).await?;
    let form = Form::parse(&kind, &data)?;
    let method = form.required("method")?;
    if !matches!(method, "band" | "ndvi" | "ndwi" | "ndbi" | "hillshade") {
        return Err(ComputeError::InvalidRequest);
    }
    let a = form.band("band_a", 1)?;
    let b = form.band("band_b", 2)?;
    let source = form.file("file")?.ok_or(ComputeError::InvalidRequest)?;
    let other = form.file("second")?;
    if other.is_some() && !matches!(method, "ndvi" | "ndwi" | "ndbi") {
        return Err(invalid(
            "Second file is supported for index calculations only",
        ));
    }
    let scratch = Scratch::create()?;
    let src = scratch.stage("source.tif", source, 32 * MIB).await?;
    let second = match other {
        Some(bytes) => scratch
            .stage("second.tif", bytes, 32 * MIB)
            .await?
            .to_string_lossy()
            .into_owned(),
        None => "-".to_owned(),
    };
    let output = scratch.path("result.tif");
    let mut value = worker(
        &scratch,
        "raster_worker.py",
        &[
            src.to_string_lossy().into_owned(),
            second,
            method.to_owned(),
            a.to_string(),
            b.to_string(),
            output.to_string_lossy().into_owned(),
        ],
        false,
        28,
        4 * MIB,
    )
    .await?;
    let result = read_output(&output, 16 * MIB).await?;
    authorize(&state, &auth, ids, false).await?.commit().await?;
    value["method"] = json!(method);
    value["tiff"] = json!(STANDARD.encode(result));
    value["persisted"] = json!(false);
    Ok(Json(value))
}

async fn georeference(
    State(state): State<AppState>,
    auth: HumanSession,
    Path(ids): Path<(Uuid, Uuid)>,
    req: Request,
) -> Result<Json<Value>> {
    use sha2::{Digest, Sha256};
    require_csrf(&state, &auth, req.headers()).map_err(CatalogError::Identity)?;
    authorize(&state, &auth, ids, false).await?.commit().await?;
    let _permit = gate()?;
    let (kind, data) = body(req, 33 * MIB).await?;
    let form = Form::parse(&kind, &data)?;
    let mode = form.required("mode")?;
    if !matches!(mode, "inspect" | "solve") {
        return Err(ComputeError::InvalidRequest);
    }
    let source = form.file("file")?.ok_or(ComputeError::InvalidRequest)?;
    let digest = hex::encode(Sha256::digest(source));
    let config = if mode == "solve" {
        let points = form.required("points")?;
        if points.len() > 16384 || form.required("source_sha256")? != digest {
            return Err(invalid("Source or GCP preview mismatch"));
        }
        let points: Value =
            serde_json::from_str(points).map_err(|_| invalid("Invalid GCP list"))?;
        let epsg: i32 = form
            .required("epsg")?
            .parse()
            .map_err(|_| ComputeError::InvalidRequest)?;
        if !matches!(epsg, 4326 | 3857)
            || !points
                .as_array()
                .is_some_and(|p| (3..=32).contains(&p.len()))
        {
            return Err(invalid("Unsupported GCP profile"));
        }
        json!({"mode":mode,"source_sha256":digest,"points":points,"epsg":epsg})
    } else {
        json!({"mode":mode})
    };
    let scratch = Scratch::create()?;
    let src = scratch.stage("source.tif", source, 32 * MIB).await?;
    let cfg = scratch.path("georef.json");
    tokio::fs::write(&cfg, config.to_string())
        .await
        .map_err(|_| invalid("Raster workspace unavailable"))?;
    let output = scratch.path("result.tif");
    let mut result = worker(
        &scratch,
        "georeference_worker.py",
        &[
            src.to_string_lossy().into_owned(),
            cfg.to_string_lossy().into_owned(),
            output.to_string_lossy().into_owned(),
        ],
        false,
        28,
        4 * MIB,
    )
    .await?;
    if result["source_sha256"].as_str() != Some(digest.as_str()) {
        return Err(invalid("Raster source digest mismatch"));
    }
    if mode == "solve" {
        let file = read_output(&output, 16 * MIB).await?;
        if !file
            .get(..4)
            .and_then(|v| v.try_into().ok())
            .is_some_and(valid_geotiff_magic)
        {
            return Err(invalid("Invalid GeoTIFF output"));
        }
        result["sha256"] = json!(hex::encode(Sha256::digest(&file)));
        result["byte_size"] = json!(file.len());
        result["tiff"] = json!(STANDARD.encode(file));
    }
    authorize(&state, &auth, ids, false).await?.commit().await?;
    result["mode"] = json!(mode);
    Ok(Json(result))
}

async fn existing(read: &mut ProjectRead<'_>, id: Uuid, missing: &str) -> Result<Vec<u8>> {
    let length = raster::file_size(read, id)
        .await?
        .ok_or_else(|| detail(StatusCode::NOT_FOUND, missing))?;
    if length > MAX_INPUT as i64 {
        return Err(too_large("Input raster exceeds 64 MiB"));
    }
    raster::file(read, id)
        .await?
        .ok_or_else(|| detail(StatusCode::NOT_FOUND, missing))
}

async fn stage_input(
    scratch: &Scratch,
    read: &mut ProjectRead<'_>,
    name: &str,
    file: Option<&[u8]>,
    id: Option<Uuid>,
    total: &mut usize,
) -> Result<String> {
    let stored;
    let input = if let Some(file) = file {
        file
    } else if let Some(id) = id {
        stored = existing(read, id, "Input raster not found in this project").await?;
        &stored
    } else {
        return Ok("-".to_owned());
    };
    *total += input.len();
    if *total > MAX_COMBINED {
        return Err(too_large("Combined rasters limited to 96 MiB"));
    }
    Ok(scratch
        .stage(name, input, MAX_INPUT)
        .await?
        .to_string_lossy()
        .into_owned())
}

async fn check_quota(read: &mut ProjectRead<'_>, extra: usize) -> Result<()> {
    let (count, size) = raster::quota(read).await?;
    if count >= 32 || size.saturating_add(extra as i64) > 512 * MIB as i64 {
        return Err(too_large("Project raster quota: 32 layers and 512 MiB"));
    }
    Ok(())
}

fn sentinel_valid(scene: &str, baseline: &str) -> bool {
    let scene_ok = scene.len() <= 100
        && ["S2A_MSIL2A_", "S2B_MSIL2A_", "S2C_MSIL2A_", "S2D_MSIL2A_"]
            .iter()
            .any(|prefix| {
                scene.strip_prefix(*prefix).is_some_and(|suffix| {
                    !suffix.is_empty()
                        && suffix
                            .bytes()
                            .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == b'_')
                })
            });
    let bytes = baseline.as_bytes();
    scene_ok
        && bytes.len() == 5
        && bytes[2] == b'.'
        && bytes
            .iter()
            .enumerate()
            .all(|(i, c)| i == 2 || c.is_ascii_digit())
}

fn parse_store_intent(form: &Form<'_>) -> Result<()> {
    let method = parse_method(form.required("method")?)?;
    let name = form.required("name")?;
    let factor = form
        .text("factor")?
        .map(|s| s.parse::<f64>().map_err(|_| ComputeError::InvalidRequest))
        .transpose()?;
    let resampling = form.text("resampling")?;
    if resampling.is_some_and(|s| !matches!(s, "near" | "bilinear")) {
        return Err(ComputeError::InvalidRequest);
    }
    let formula = form.text("formula")?;
    let clip = form.text("clip_geojson")?;
    if formula.is_some_and(|s| s.chars().count() > 200)
        || clip.is_some_and(|s| s.chars().count() > 150_000)
    {
        return Err(ComputeError::InvalidRequest);
    }
    let scene = form.text("sentinel_scene")?;
    let baseline = form.text("sentinel_baseline")?;
    if let (Some(scene), Some(baseline)) = (scene, baseline) {
        if !sentinel_valid(scene, baseline) {
            return Err(ComputeError::InvalidRequest);
        }
    }
    validate_store_intent(StoreIntent {
        method,
        name,
        primary_file: form.file("file")?.is_some(),
        primary_id: form.id("source_id")?.is_some(),
        second_file: form.file("second")?.is_some(),
        second_id: form.id("second_id")?.is_some(),
        third_file: form.file("third")?.is_some(),
        third_id: form.id("third_id")?.is_some(),
        formula: formula.is_some(),
        clip_geojson: clip.is_some(),
        factor,
        resampling: resampling.is_some(),
        sentinel_scene: scene.is_some(),
        sentinel_baseline: baseline.is_some(),
        band_a: form.band("band_a", 1)?,
        band_b: form.band("band_b", 2)?,
    })
    .map_err(|e| invalid(e.to_string()))?;
    Ok(())
}

async fn read_tiles(scratch: &Scratch, manifest: Value, cog_len: usize) -> Result<Vec<RasterTile>> {
    let entries = manifest
        .as_array()
        .filter(|v| (1..=192).contains(&v.len()))
        .ok_or_else(|| invalid("Invalid raster tile manifest"))?;
    let mut seen = HashSet::new();
    let mut tiles = Vec::with_capacity(entries.len());
    let mut size = cog_len;
    for entry in entries {
        let coords = entry
            .as_array()
            .filter(|v| v.len() == 3)
            .ok_or_else(|| invalid("Invalid raster tile"))?;
        let mut values = [0_i32; 3];
        for (i, coord) in coords.iter().enumerate() {
            values[i] = coord
                .as_i64()
                .and_then(|v| i32::try_from(v).ok())
                .ok_or_else(|| invalid("Invalid raster tile"))?;
        }
        let [z, x, y] = values;
        if !(0..=20).contains(&z)
            || !(0..(1_i32 << z)).contains(&x)
            || !(0..(1_i32 << z)).contains(&y)
            || !seen.insert((z, x, y))
        {
            return Err(invalid("Invalid raster tile"));
        }
        let png = read_output(&scratch.path(&format!("{z}-{x}-{y}.png")), 400_000).await?;
        if !png.starts_with(b"\x89PNG\r\n\x1a\n") {
            return Err(invalid("Invalid raster PNG"));
        }
        size += png.len();
        if size > MAX_OUTPUT {
            return Err(too_large("Raster and tiles exceed 128 MiB"));
        }
        tiles.push(RasterTile { z, x, y, png });
    }
    Ok(tiles)
}

async fn store(
    State(state): State<AppState>,
    auth: HumanSession,
    Path(ids): Path<(Uuid, Uuid)>,
    req: Request,
) -> Result<(StatusCode, Json<Value>)> {
    require_csrf(&state, &auth, req.headers()).map_err(CatalogError::Identity)?;
    let mut edit = authorize(&state, &auth, ids, true).await?;
    check_quota(&mut edit, 0).await?;
    edit.commit().await?;
    let _permit = gate()?;
    let (kind, data) = body(req, MAX_COMBINED + MIB).await?;
    let form = Form::parse(&kind, &data)?;
    parse_store_intent(&form)?;
    let method = form.required("method")?;
    let name = form.required("name")?.trim().to_owned();
    let a = form.band("band_a", 1)?;
    let b = form.band("band_b", 2)?;
    let c = form.band("band_c", 3)?;
    let source_ids = [
        form.id("source_id")?,
        form.id("second_id")?,
        form.id("third_id")?,
    ];
    let files = [
        form.file("file")?,
        form.file("second")?,
        form.file("third")?,
    ];
    let mut read = authorize(&state, &auth, ids, true).await?;
    let scratch = Scratch::create()?;
    let mut total = 0;
    let mut staged = Vec::new();
    for (i, file_name) in ["source.tif", "second.tif", "third.tif"].iter().enumerate() {
        staged.push(
            stage_input(
                &scratch,
                &mut read,
                file_name,
                files[i],
                source_ids[i],
                &mut total,
            )
            .await?,
        );
    }
    read.commit().await?;
    let clip = form
        .text("clip_geojson")?
        .map(|s| serde_json::from_str::<Value>(s).map_err(|_| invalid("Invalid clip GeoJSON")))
        .transpose()?;
    let factor = form
        .text("factor")?
        .map(|s| s.parse::<f64>().map_err(|_| ComputeError::InvalidRequest))
        .transpose()?;
    let formula = form.text("formula")?;
    let resampling = form.text("resampling")?;
    let scene = form.text("sentinel_scene")?;
    let baseline = form.text("sentinel_baseline")?;
    let config = scratch.path("config.json");
    tokio::fs::write(
        &config,
        json!({
            "formula": formula, "clip": clip, "third": staged[2], "band_c": c,
            "factor": factor, "resampling": resampling, "sentinel_baseline": baseline,
        })
        .to_string(),
    )
    .await
    .map_err(|_| invalid("Raster workspace unavailable"))?;
    let mut metadata = worker(
        &scratch,
        "persistent_raster_worker.py",
        &[
            staged[0].clone(),
            staged[1].clone(),
            method.to_owned(),
            a.to_string(),
            b.to_string(),
            scratch.0.to_string_lossy().into_owned(),
            config.to_string_lossy().into_owned(),
        ],
        true,
        65,
        4 * MIB,
    )
    .await?;
    let manifest = metadata
        .as_object_mut()
        .and_then(|m| m.remove("tiles"))
        .ok_or_else(|| invalid("Invalid raster tile manifest"))?;
    let cog = read_output(&scratch.path("result.tif"), MAX_INPUT).await?;
    let magic = cog.get(..4).and_then(|v| v.try_into().ok());
    if !magic.is_some_and(valid_geotiff_magic) {
        return Err(invalid("Invalid GeoTIFF result"));
    }
    let tiles = read_tiles(&scratch, manifest, cog.len()).await?;
    let size = cog.len() + tiles.iter().map(|t| t.png.len()).sum::<usize>();
    metadata["tile_count"] = json!(tiles.len());
    metadata["inputs"] = json!({
        "a": source_ids[0], "b": source_ids[1], "c": source_ids[2],
        "band_a": a, "band_b": b, "band_c": c,
    });
    if let Some(scene) = scene {
        metadata["sentinel"] = json!({
            "scene": scene, "baseline": baseline, "mask": "SCL 4,5,6",
            "provider": "Microsoft Planetary Computer", "input": "client crop",
        });
    }
    if let Some(factor) = factor {
        metadata["factor"] = json!(factor);
        metadata["resampling"] = json!(resampling);
    }
    if let Some(formula) = formula {
        metadata["formula"] = json!(formula);
    }
    if let Some(clip) = clip {
        metadata["clip_geojson"] = clip;
    }
    let mut edit = authorize(&state, &auth, ids, true).await?;
    check_quota(&mut edit, size).await?;
    for id in source_ids.into_iter().flatten() {
        if raster::file_size(&mut edit, id).await?.is_none() {
            return Err(detail(
                StatusCode::CONFLICT,
                "Input raster was removed during processing",
            ));
        }
    }
    let value = raster::insert_layer(
        &mut edit,
        NewRaster {
            name,
            metadata,
            cog,
            tiles,
            actor_id: auth.actor_id(),
            actor_display: auth.display_name().to_owned(),
        },
    )
    .await?;
    edit.commit().await?;
    Ok((StatusCode::CREATED, Json(value)))
}

async fn report(
    State(state): State<AppState>,
    auth: HumanSession,
    Path(ids): Path<(Uuid, Uuid)>,
    req: Request,
) -> Result<Json<Value>> {
    require_csrf(&state, &auth, req.headers()).map_err(CatalogError::Identity)?;
    let mut read = authorize(&state, &auth, ids, false).await?;
    let _permit = gate()?;
    let (kind, bytes) = body(req, MIB).await?;
    let form = Form::parse(&kind, &bytes)?;
    let method = form.required("method")?;
    let id = form.id("source_id")?.ok_or(ComputeError::InvalidRequest)?;
    let band = form.band("band", 1)?;
    let zones = form.text("zones_json")?;
    let interval = form.text("interval")?;
    if !matches!(method, "zonal" | "contour")
        || zones.is_some() != (method == "zonal")
        || interval.is_some() != (method == "contour")
    {
        return Err(invalid("Provide zones or contour interval"));
    }
    let zones = zones
        .map(|s| {
            if s.chars().count() > 750_000 {
                return Err(invalid("Invalid report parameters"));
            }
            let value: Value =
                serde_json::from_str(s).map_err(|_| invalid("Invalid report parameters"))?;
            if !value
                .as_array()
                .is_some_and(|v| (1..=100).contains(&v.len()) && v.iter().all(Value::is_object))
            {
                return Err(invalid("Invalid report parameters"));
            }
            Ok(value)
        })
        .transpose()?;
    let interval = interval
        .map(|s| {
            s.parse::<f64>()
                .ok()
                .filter(|x| x.is_finite() && *x > 0.0)
                .ok_or_else(|| invalid("Invalid report parameters"))
        })
        .transpose()?;
    let data = existing(&mut read, id, "Input raster not found in this project").await?;
    read.commit().await?;
    let scratch = Scratch::create()?;
    let source = scratch.stage("source.tif", &data, MAX_INPUT).await?;
    let config = scratch.path("config.json");
    tokio::fs::write(
        &config,
        json!({"zones": zones, "interval": interval}).to_string(),
    )
    .await
    .map_err(|_| invalid("Raster workspace unavailable"))?;
    let result = worker(
        &scratch,
        "persistent_raster_worker.py",
        &[
            source.to_string_lossy().into_owned(),
            "-".to_owned(),
            method.to_owned(),
            band.to_string(),
            "1".to_owned(),
            scratch.0.to_string_lossy().into_owned(),
            config.to_string_lossy().into_owned(),
        ],
        true,
        65,
        4_000_000,
    )
    .await?;
    let mut read = authorize(&state, &auth, ids, false).await?;
    if raster::file_size(&mut read, id).await?.is_none() {
        return Err(detail(
            StatusCode::CONFLICT,
            "Input raster removed during processing",
        ));
    }
    read.commit().await?;
    Ok(Json(result))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct TerrainProfile {
    elevation_unit: String,
    vertical_reference: String,
    reference_label: String,
    acknowledge_dem: bool,
}

async fn terrain(
    State(state): State<AppState>,
    auth: HumanSession,
    headers: HeaderMap,
    Path((org, project, layer)): Path<(Uuid, Uuid, Uuid)>,
    input: std::result::Result<Json<TerrainProfile>, axum::extract::rejection::JsonRejection>,
) -> Result<Response> {
    require_csrf(&state, &auth, &headers).map_err(CatalogError::Identity)?;
    let Json(input) = input.map_err(|_| ComputeError::InvalidRequest)?;
    if input.elevation_unit != "metre"
        || !input.acknowledge_dem
        || !matches!(
            input.vertical_reference.as_str(),
            "local" | "orthometric" | "ellipsoidal"
        )
        || !(1..=120).contains(&input.reference_label.chars().count())
    {
        return Err(ComputeError::InvalidRequest);
    }
    let label = input.reference_label.trim();
    if label.is_empty() {
        return Err(invalid("Declare the vertical reference"));
    }
    let _permit = gate()?;
    let mut read = authorize(&state, &auth, (org, project), false).await?;
    let (name, metadata) = raster::metadata(&mut read, layer)
        .await?
        .ok_or_else(|| detail(StatusCode::NOT_FOUND, "DEM layer not found"))?;
    if metadata["method"] != "band"
        || metadata.get("output_bands").unwrap_or(&json!(1)) != &json!(1)
    {
        return Err(invalid(
            "Select an imported single elevation band, not an index",
        ));
    }
    let data = existing(&mut read, layer, "DEM layer not found").await?;
    read.commit().await?;
    let scratch = Scratch::create()?;
    let source = scratch.stage("source.tif", &data, MAX_INPUT).await?;
    let mut result = worker(
        &scratch,
        "terrain_worker.py",
        &[source.to_string_lossy().into_owned()],
        false,
        28,
        1_000_000,
    )
    .await?;
    let mut read = authorize(&state, &auth, (org, project), false).await?;
    if raster::file_size(&mut read, layer).await?.is_none() {
        return Err(detail(
            StatusCode::CONFLICT,
            "DEM was deleted during sampling",
        ));
    }
    read.commit().await?;
    result["source_id"] = json!(layer);
    result["name"] = json!(name);
    result["elevation_unit"] = json!("metre");
    result["vertical_reference"] = json!(input.vertical_reference);
    result["reference_label"] = json!(label);
    result["render_origin_m"] = result["min_m"].clone();
    result["declared_by_user"] = json!(true);
    let mut response = Json(result).into_response();
    response.headers_mut().insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("private, no-store"),
    );
    response.headers_mut().insert(
        "x-content-type-options",
        HeaderValue::from_static("nosniff"),
    );
    Ok(response)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sentinel_profile_is_bounded() {
        assert!(sentinel_valid("S2A_MSIL2A_20260101", "05.09"));
        assert!(!sentinel_valid("S2A_MSIL2A_../", "05.09"));
        assert!(!sentinel_valid("S2A_MSIL2A_20260101", "5.9"));
    }
}
