//! Read-only OSM provider gateway. Only operator configuration selects destinations.
use crate::{identity::HumanSession, ApiError, AppState};
use axum::{extract::Query, routing::get, Json, Router};
use ovg_core::AppError;
use serde::Deserialize;
use serde_json::{json, Value};
use std::{process::Stdio, sync::LazyLock, time::Duration};
use tokio::{io::AsyncReadExt, process::Command, sync::Semaphore};
use url::Url;

static REQUESTS: LazyLock<Semaphore> = LazyLock::new(|| Semaphore::new(8));
const MAX_BODY: u64 = 2 * 1024 * 1024;
fn invalid(message: &str) -> ApiError {
    AppError::validation("INVALID_NAVIGATION_INPUT", message).into()
}
fn unavailable() -> ApiError {
    AppError::service_unavailable(
        "NAVIGATION_PROVIDER_UNAVAILABLE",
        "Navigation provider is not configured or temporarily unavailable.",
    )
    .into()
}
fn point(lon: f64, lat: f64) -> bool {
    lon.is_finite() && lat.is_finite() && lon.abs() <= 180.0 && lat.abs() <= 90.0
}
fn endpoint(env: &str, path: &str) -> Result<Url, ApiError> {
    let raw = std::env::var(env).map_err(|_| unavailable())?;
    let mut url = Url::parse(&raw).map_err(|_| unavailable())?;
    if !matches!(url.scheme(), "http" | "https")
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return Err(unavailable());
    }
    url.set_path(path);
    Ok(url)
}
async fn request(url: Url) -> Result<Value, ApiError> {
    let _permit = REQUESTS
        .try_acquire()
        .map_err(|_| ApiError::from(AppError::rate_limited(Some(1))))?;
    let mut child = Command::new("/usr/bin/curl")
        .args([
            "-q",
            "--silent",
            "--fail",
            "--globoff",
            "--proto",
            "=http,https",
            "--connect-timeout",
            "2",
            "--max-time",
            "8",
            "--max-filesize",
            "2097152",
            "--url",
            url.as_str(),
        ])
        .env_clear()
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .stdout(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .map_err(|_| unavailable())?;
    let stdout = child.stdout.take().ok_or_else(unavailable)?;
    let data = tokio::time::timeout(Duration::from_secs(9), async {
        let mut body = Vec::new();
        stdout.take(MAX_BODY + 1).read_to_end(&mut body).await?;
        if body.len() as u64 > MAX_BODY {
            return Ok::<_, std::io::Error>(None);
        }
        Ok(if child.wait().await?.success() {
            Some(body)
        } else {
            None
        })
    })
    .await
    .map_err(|_| unavailable())?
    .map_err(|_| unavailable())?
    .ok_or_else(unavailable)?;
    serde_json::from_slice(&data).map_err(|_| unavailable())
}
#[derive(Deserialize)]
struct Search {
    q: String,
    lon: Option<f64>,
    lat: Option<f64>,
}
#[derive(Deserialize)]
struct Reverse {
    lon: f64,
    lat: f64,
}
#[derive(Deserialize)]
struct Route {
    from_lon: f64,
    from_lat: f64,
    to_lon: f64,
    to_lat: f64,
    profile: Option<String>,
}
fn label(p: &Value) -> String {
    let parts = [
        "name",
        "housenumber",
        "street",
        "district",
        "city",
        "county",
        "state",
        "country",
    ]
    .iter()
    .filter_map(|key| p.get(*key).and_then(Value::as_str))
    .filter(|s| !s.is_empty());
    let mut values = Vec::new();
    for part in parts {
        if !values.contains(&part) {
            values.push(part);
        }
    }
    values.join(", ")
}
fn normalize_places(v: Value) -> Result<Json<Value>, ApiError> {
    let features = v
        .get("features")
        .and_then(Value::as_array)
        .ok_or_else(unavailable)?;
    let items: Vec<Value> = features.iter().take(20).filter_map(|f| {
        let c = f.pointer("/geometry/coordinates")?.as_array()?;
        let lon = c.first()?.as_f64()?; let lat = c.get(1)?.as_f64()?;
        if !point(lon,lat) { return None; }
        let p = f.get("properties")?;
        Some(json!({"label":label(p),"point":[lon,lat],"extent":p.get("extent"),"osm_id":p.get("osm_id"),"osm_type":p.get("osm_type"),"source":"OpenStreetMap / Photon"}))
    }).collect();
    Ok(Json(
        json!({"items":items,"crs":"EPSG:4326","attribution":"© OpenStreetMap contributors"}),
    ))
}
async fn search(_auth: HumanSession, Query(q): Query<Search>) -> Result<Json<Value>, ApiError> {
    let term = q.q.trim();
    if term.chars().count() < 2 || term.chars().count() > 200 || term.chars().any(char::is_control)
    {
        return Err(invalid("Search must contain 2–200 characters."));
    }
    let mut url = endpoint("OVG_PHOTON_URL", "/api/")?;
    url.query_pairs_mut()
        .append_pair("q", term)
        .append_pair("limit", "10");
    match (q.lon, q.lat) {
        (Some(lon), Some(lat)) if point(lon, lat) => {
            url.query_pairs_mut()
                .append_pair("lon", &lon.to_string())
                .append_pair("lat", &lat.to_string());
        }
        (None, None) => {}
        _ => return Err(invalid("Invalid search bias coordinates.")),
    }
    normalize_places(request(url).await?)
}
async fn reverse(_auth: HumanSession, Query(q): Query<Reverse>) -> Result<Json<Value>, ApiError> {
    if !point(q.lon, q.lat) {
        return Err(invalid("Invalid coordinates."));
    }
    let mut url = endpoint("OVG_PHOTON_URL", "/reverse")?;
    url.query_pairs_mut()
        .append_pair("lon", &q.lon.to_string())
        .append_pair("lat", &q.lat.to_string())
        .append_pair("limit", "5");
    normalize_places(request(url).await?)
}
async fn route(_auth: HumanSession, Query(q): Query<Route>) -> Result<Json<Value>, ApiError> {
    if !point(q.from_lon, q.from_lat) || !point(q.to_lon, q.to_lat) {
        return Err(invalid("Invalid route coordinates."));
    }
    let profile = q.profile.as_deref().unwrap_or("auto");
    if !matches!(
        profile,
        "auto" | "motorcycle" | "motor_scooter" | "pedestrian" | "bicycle" | "truck"
    ) {
        return Err(invalid("Unsupported routing profile."));
    }
    let mut url = endpoint("OVG_VALHALLA_URL", "/route")?;
    let payload = json!({"locations":[{"lon":q.from_lon,"lat":q.from_lat},{"lon":q.to_lon,"lat":q.to_lat}],"costing":profile,"units":"kilometers","language":"en-US","shape_format":"polyline6"});
    url.query_pairs_mut()
        .append_pair("json", &payload.to_string());
    let v = request(url).await?;
    if !v.get("trip").is_some_and(Value::is_object) {
        return Err(unavailable());
    }
    Ok(Json(
        json!({"route":v,"crs":"EPSG:4326","shape_format":"polyline6","attribution":"© OpenStreetMap contributors"}),
    ))
}
pub(crate) fn router() -> Router<AppState> {
    Router::new()
        .route("/search/suggest", get(search))
        .route("/search", get(search))
        .route("/geocode", get(search))
        .route("/geocode/reverse", get(reverse))
        .route("/routing/route", get(route))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn coordinate_bounds() {
        assert!(point(106.8, 10.9));
        assert!(!point(181.0, 0.0));
        assert!(!point(f64::NAN, 0.0));
    }
    #[test]
    fn place_normalization() {
        let Json(v)=normalize_places(json!({"features":[{"geometry":{"coordinates":[106.8,10.9]},"properties":{"name":"Biên Hòa","city":"Biên Hòa","country":"Việt Nam"}},{"geometry":{"coordinates":[999,0]},"properties":{}}]})).unwrap();
        assert_eq!(v["items"].as_array().unwrap().len(), 1);
        assert_eq!(v["items"][0]["label"], "Biên Hòa, Việt Nam");
    }
}
