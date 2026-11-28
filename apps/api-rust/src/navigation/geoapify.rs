//! Hosted backend adapter. Never return provider request metadata or raw errors.
use super::{invalid, point, request_status, unavailable, ApiError, UpstreamError};
use axum::Json;
use serde_json::{json, Value};
use std::{
    collections::HashMap,
    sync::{LazyLock, Mutex},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use url::Url;

const ATTRIBUTION: &str = "Powered by Geoapify | © OpenStreetMap contributors";
const SOURCE: &str = "Geoapify / OpenStreetMap";
const CACHE_CAPACITY: usize = 256;
struct Limits {
    second: Instant,
    count: u32,
    day: u64,
    daily: u32,
    auth_blocked: [Option<Instant>; 2],
    quota_blocked: Option<Instant>,
    cache: HashMap<String, (Instant, Value)>,
}
static LIMITS: LazyLock<Mutex<Limits>> = LazyLock::new(|| {
    Mutex::new(Limits {
        second: Instant::now(),
        count: 0,
        day: 0,
        daily: 0,
        auth_blocked: [None; 2],
        quota_blocked: None,
        cache: HashMap::new(),
    })
});

pub(super) fn enabled() -> Result<bool, ApiError> {
    match std::env::var("OVG_NAVIGATION_PROVIDER")
        .as_deref()
        .unwrap_or("self-hosted")
    {
        "geoapify" => Ok(true),
        "self-hosted" => Ok(false),
        _ => Err(unavailable()),
    }
}
fn key(slot: usize) -> Result<String, ApiError> {
    let (file_var, env_var) = if slot == 0 {
        ("OVG_GEOAPIFY_API_KEY_FILE", "OVG_GEOAPIFY_API_KEY")
    } else {
        (
            "OVG_GEOAPIFY_BACKUP_API_KEY_FILE",
            "OVG_GEOAPIFY_BACKUP_API_KEY",
        )
    };
    let raw = if let Some(path) = std::env::var(file_var).ok().filter(|p| !p.is_empty()) {
        let meta = std::fs::metadata(&path).map_err(|_| unavailable())?;
        if !meta.is_file() || meta.len() > 1024 {
            return Err(unavailable());
        }
        std::fs::read_to_string(path).map_err(|_| unavailable())?
    } else {
        std::env::var(env_var).map_err(|_| unavailable())?
    };
    let key = raw.trim();
    if key.is_empty()
        || key.len() > 256
        || !key
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || c == b'-' || c == b'_')
    {
        return Err(unavailable());
    }
    Ok(key.to_owned())
}
fn origin(path: &str) -> Result<Url, ApiError> {
    // Fixed TLS origin: a configuration typo cannot send the credential elsewhere.
    Url::parse(&format!("https://api.geoapify.com/v1/{path}")).map_err(|_| unavailable())
}
fn admit(limits: &mut Limits, now: Instant, day: u64, daily_max: u32, cost: u32) -> bool {
    if limits.day != day {
        limits.day = day;
        limits.daily = 0;
    }
    if now.duration_since(limits.second) >= Duration::from_secs(1) {
        limits.second = now;
        limits.count = 0;
    }
    if limits.count >= 4 || limits.daily.saturating_add(cost) > daily_max {
        return false;
    }
    limits.count += 1;
    limits.daily += cost;
    true
}
fn eligible(limits: &Limits, slot: usize, now: Instant) -> bool {
    !limits.auth_blocked[slot].is_some_and(|until| until > now)
}
fn record_failure(limits: &mut Limits, slot: usize, status: u16, now: Instant) {
    match status {
        401 => limits.auth_blocked[slot] = Some(now + Duration::from_secs(300)),
        429 => limits.quota_blocked = Some(now + Duration::from_secs(60)),
        _ => {}
    }
}
fn quota_open(limits: &Limits, now: Instant) -> bool {
    !limits.quota_blocked.is_some_and(|until| until > now)
}
async fn fetch(url: Url, route: bool) -> Result<Json<Value>, ApiError> {
    // Shared across both credentials: no quota-based key rotation.
    let cache_key = url.as_str().to_owned();
    {
        let mut limits = LIMITS.lock().map_err(|_| unavailable())?;
        let now = Instant::now();
        limits.cache.retain(|_, (expires, _)| *expires > now);
        if let Some((_, value)) = limits.cache.get(&cache_key) {
            return Ok(Json(value.clone()));
        }
        if !quota_open(&limits, now) {
            return Err(ovg_core::AppError::rate_limited(Some(60)).into());
        }
    }
    let primary = key(0).ok();
    let standby = key(1).ok().filter(|s| Some(s) != primary.as_ref());
    let daily_max = std::env::var("OVG_GEOAPIFY_MAX_REQUESTS_PER_DAY")
        .ok()
        .and_then(|n| n.parse::<u32>().ok())
        .unwrap_or(2000)
        .min(2000);
    let day = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| unavailable())?
        .as_secs()
        / 86400;
    let cost = if route { 10 } else { 1 };
    for (slot, secret) in [primary, standby].into_iter().enumerate() {
        let Some(secret) = secret else { continue };
        {
            let mut limits = LIMITS.lock().map_err(|_| unavailable())?;
            let now = Instant::now();
            if !quota_open(&limits, now) {
                return Err(ovg_core::AppError::rate_limited(Some(60)).into());
            }
            if !eligible(&limits, slot, now) {
                continue;
            }
            if !admit(&mut limits, now, day, daily_max, cost) {
                return Err(ovg_core::AppError::rate_limited(Some(1)).into());
            }
        }
        let mut target = url.clone();
        target.query_pairs_mut().append_pair("apiKey", &secret);
        let raw = match request_status(target).await {
            Ok(v) => v,
            Err(UpstreamError::Http(status)) => {
                {
                    let mut limits = LIMITS.lock().map_err(|_| unavailable())?;
                    record_failure(&mut limits, slot, status, Instant::now());
                }
                if status == 401 {
                    continue;
                }
                // 429 is ambiguous (rate or daily quota) and never rotates credentials.
                return Err(UpstreamError::Http(status).into_api());
            }
            Err(e) => return Err(e.into_api()),
        };
        let value = if route {
            normalize_route(&raw)?
        } else {
            normalize_places(&raw)?
        };
        let mut limits = LIMITS.lock().map_err(|_| unavailable())?;
        if limits.cache.len() >= CACHE_CAPACITY {
            limits.cache.clear();
        }
        let ttl = if route { 60 } else { 300 };
        limits.cache.insert(
            cache_key,
            (Instant::now() + Duration::from_secs(ttl), value.clone()),
        );
        return Ok(Json(value));
    }
    Err(unavailable())
}
pub(super) async fn places(
    path: &str,
    term: Option<&str>,
    bias: Option<(f64, f64)>,
) -> Result<Json<Value>, ApiError> {
    let mut url = origin(path)?;
    url.query_pairs_mut()
        .append_pair("format", "geojson")
        .append_pair("lang", "vi")
        .append_pair("limit", "10");
    if let Some(term) = term {
        url.query_pairs_mut()
            .append_pair("text", term)
            .append_pair("filter", "countrycode:vn");
        let preference = if let Some((lon, lat)) = bias {
            format!("proximity:{lon},{lat}")
        } else {
            "countrycode:vn".to_owned()
        };
        url.query_pairs_mut().append_pair("bias", &preference);
    } else if let Some((lon, lat)) = bias {
        url.query_pairs_mut()
            .append_pair("lon", &lon.to_string())
            .append_pair("lat", &lat.to_string());
    } else {
        return Err(invalid("Reverse coordinates are required."));
    }
    fetch(url, false).await
}
fn route_url(locations: &[Value], profile: &str, alternatives: u8) -> Result<Url, ApiError> {
    let mode = match profile {
        "auto" => "drive",
        "motorcycle" => "motorcycle",
        "motor_scooter" => "scooter",
        "pedestrian" => "walk",
        "bicycle" => "bicycle",
        "truck" => "truck",
        _ => return Err(invalid("Unsupported routing profile.")),
    };
    // Public contract is lon,lat; Geoapify's default waypoint syntax is lat,lon.
    let waypoints = locations
        .iter()
        .map(|p| format!("{},{}", p["lat"], p["lon"]))
        .collect::<Vec<_>>()
        .join("|");
    let mut url = origin("routing")?;
    url.query_pairs_mut()
        .append_pair("waypoints", &waypoints)
        .append_pair("mode", mode)
        .append_pair("format", "geojson")
        .append_pair("units", "metric")
        .append_pair("lang", "en")
        .append_pair("details", "instruction_details");
    if alternatives > 0 {
        return Err(invalid(
            "Alternative routes are not available with this provider.",
        ));
    }
    Ok(url)
}
pub(super) async fn route(
    locations: &[Value],
    profile: &str,
    alternatives: u8,
) -> Result<Json<Value>, ApiError> {
    fetch(route_url(locations, profile, alternatives)?, true).await
}
fn extent(v: &Value) -> Option<Value> {
    let b = v.as_array()?;
    if b.len() != 4 {
        return None;
    }
    let n = b.iter().map(Value::as_f64).collect::<Option<Vec<_>>>()?;
    (point(n[0], n[1]) && point(n[2], n[3]) && n[0] <= n[2] && n[1] <= n[3]).then(|| json!(n))
}
fn normalize_places(v: &Value) -> Result<Value, ApiError> {
    let features = v
        .get("features")
        .and_then(Value::as_array)
        .ok_or_else(unavailable)?;
    let items: Vec<Value> = features
        .iter()
        .take(20)
        .filter_map(|f| {
            if f.pointer("/geometry/type")?.as_str()? != "Point" {
                return None;
            }
            let c = f.pointer("/geometry/coordinates")?.as_array()?;
            let (lon, lat) = (c.first()?.as_f64()?, c.get(1)?.as_f64()?);
            if !point(lon, lat) {
                return None;
            }
            let p = f.get("properties")?;
            let label = p
                .get("formatted")
                .and_then(Value::as_str)
                .filter(|s| !s.is_empty())?;
            let id = p.pointer("/datasource/raw/osm_id").and_then(Value::as_u64);
            let osm_type = p
                .pointer("/datasource/raw/osm_type")
                .and_then(Value::as_str)
                .filter(|s| matches!(*s, "node" | "way" | "relation" | "N" | "W" | "R"));
            Some(
                json!({"label":label,"point":[lon,lat],"extent":f.get("bbox").and_then(extent),
            "osm_id":id,"osm_type":osm_type,"source":SOURCE}),
            )
        })
        .collect();
    Ok(json!({"items":items,"crs":"EPSG:4326","attribution":ATTRIBUTION}))
}
fn nonnegative(v: &Value, field: &str) -> Result<f64, ApiError> {
    v.get(field)
        .and_then(Value::as_f64)
        .filter(|n| n.is_finite() && *n >= 0.0)
        .ok_or_else(unavailable)
}
fn shape(line: &Value) -> Result<String, ApiError> {
    let coords = line
        .as_array()
        .filter(|a| a.len() >= 2)
        .ok_or_else(unavailable)?;
    let (mut lat, mut lon) = (0_i64, 0_i64);
    let mut result = String::new();
    for p in coords {
        let x = p.get(0).and_then(Value::as_f64).ok_or_else(unavailable)?;
        let y = p.get(1).and_then(Value::as_f64).ok_or_else(unavailable)?;
        if !point(x, y) {
            return Err(unavailable());
        }
        let (next_lat, next_lon) = ((y * 1e6).round() as i64, (x * 1e6).round() as i64);
        for delta in [next_lat - lat, next_lon - lon] {
            let mut n = if delta < 0 { !(delta << 1) } else { delta << 1 } as u64;
            while n >= 32 {
                result.push(char::from(((n & 31) | 32) as u8 + 63));
                n >>= 5;
            }
            result.push(char::from(n as u8 + 63));
        }
        lat = next_lat;
        lon = next_lon;
    }
    Ok(result)
}
fn normalize_route(v: &Value) -> Result<Value, ApiError> {
    let features = v
        .get("features")
        .and_then(Value::as_array)
        .filter(|a| !a.is_empty())
        .ok_or_else(unavailable)?;
    let mut trips = Vec::new();
    for f in features.iter().take(3) {
        if f.pointer("/geometry/type").and_then(Value::as_str) != Some("MultiLineString") {
            return Err(unavailable());
        }
        let lines = f
            .pointer("/geometry/coordinates")
            .and_then(Value::as_array)
            .ok_or_else(unavailable)?;
        let p = f.get("properties").ok_or_else(unavailable)?;
        let route_legs = p
            .get("legs")
            .and_then(Value::as_array)
            .filter(|a| !a.is_empty() && a.len() == lines.len())
            .ok_or_else(unavailable)?;
        let mut legs = Vec::new();
        for (leg, line) in route_legs.iter().zip(lines) {
            let steps = leg
                .get("steps")
                .and_then(Value::as_array)
                .ok_or_else(unavailable)?;
            let mut maneuvers = Vec::new();
            for step in steps {
                let length = nonnegative(step, "distance")? / 1000.0;
                let time = nonnegative(step, "time")?;
                if let Some(text) = step.pointer("/instruction/text").and_then(Value::as_str) {
                    maneuvers.push(json!({"instruction":text,"length":length,"time":time}));
                }
            }
            legs.push(json!({"shape":shape(line)?,"maneuvers":maneuvers}));
        }
        trips.push(json!({"summary":{"length":nonnegative(p,"distance")? / 1000.0,"time":nonnegative(p,"time")?},"legs":legs}));
    }
    let trip = trips.remove(0);
    let alternates: Vec<Value> = trips.into_iter().map(|trip| json!({"trip":trip})).collect();
    // Deliberately whitelist fields: the upstream root properties can echo apiKey.
    Ok(
        json!({"route":{"trip":trip,"alternates":alternates},"crs":"EPSG:4326","shape_format":"polyline6","source":SOURCE,"attribution":ATTRIBUTION}),
    )
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn places_whitelist_coordinates_and_attribution() {
        let v = normalize_places(&json!({"query":{"apiKey":"private-sentinel"},"features":[
            {"geometry":{"type":"Point","coordinates":[106.8,10.9]},"bbox":[106.7,10.8,106.9,11.0],
             "properties":{"formatted":"Biên Hòa, Việt Nam","apiKey":"private-sentinel","datasource":{"raw":{"osm_id":42,"osm_type":"node"}}}},
            {"geometry":{"type":"Point","coordinates":[999,10]},"properties":{"formatted":"bad"}}
        ]})).unwrap();
        assert_eq!(v["items"].as_array().unwrap().len(), 1);
        assert_eq!(v["items"][0]["point"], json!([106.8, 10.9]));
        assert_eq!(v["items"][0]["extent"], json!([106.7, 10.8, 106.9, 11.0]));
        assert_eq!(v["items"][0]["osm_id"], 42);
        assert!(!v.to_string().contains("private-sentinel"));
        assert!(v["attribution"].as_str().unwrap().contains("Geoapify"));
        assert!(normalize_places(&json!({"error":"private-sentinel"})).is_err());
        assert_eq!(
            normalize_places(&json!({"features":[]})).unwrap()["items"],
            json!([])
        );
    }
    #[test]
    fn routing_modes_waypoint_order_and_alternatives() {
        for (profile, mode) in [
            ("auto", "drive"),
            ("motorcycle", "motorcycle"),
            ("motor_scooter", "scooter"),
            ("pedestrian", "walk"),
            ("bicycle", "bicycle"),
            ("truck", "truck"),
        ] {
            let url = route_url(
                &[
                    json!({"lon":106.8,"lat":10.9}),
                    json!({"lon":106.9,"lat":11.0}),
                    json!({"lon":107.0,"lat":11.1}),
                ],
                profile,
                0,
            )
            .unwrap();
            let pairs: HashMap<_, _> = url.query_pairs().into_owned().collect();
            assert_eq!(pairs["mode"], mode);
            assert_eq!(pairs["waypoints"], "10.9,106.8|11.0,106.9|11.1,107.0");
            assert!(!pairs.contains_key("max_solutions"));
            assert!(!pairs.contains_key("apiKey"));
        }
        assert!(route_url(&[], "invalid", 0).is_err());
    }
    fn route_fixture() -> Value {
        json!({"properties":{"apiKey":"private-sentinel"},"features":[{"geometry":{"type":"MultiLineString","coordinates":[[[0,0],[-0.000001,-0.000001]],[[106.8,10.9],[106.9,11.0]]]},
        "properties":{"distance":1500,"time":180,"legs":[{"steps":[{"distance":500,"time":60,"instruction":{"text":"Turn right"}}]},{"steps":[{"distance":1000,"time":120,"instruction":{"text":"Arrive"}}]}]}}]})
    }
    #[test]
    fn route_units_legs_polyline_and_no_raw_metadata() {
        let mut fixture = route_fixture();
        let alt = fixture["features"][0].clone();
        fixture["features"].as_array_mut().unwrap().push(alt);
        let v = normalize_route(&fixture).unwrap();
        assert_eq!(
            v["route"]["trip"]["summary"],
            json!({"length":1.5,"time":180.0})
        );
        assert_eq!(v["route"]["trip"]["legs"][0]["shape"], "??@@");
        assert_eq!(v["route"]["trip"]["legs"][0]["maneuvers"][0]["length"], 0.5);
        assert_eq!(v["route"]["alternates"].as_array().unwrap().len(), 1);
        assert!(!v.to_string().contains("private-sentinel"));
        fixture["features"][0]["geometry"]["coordinates"][0][0][0] = json!(999);
        assert!(normalize_route(&fixture).is_err());
        assert!(normalize_route(&json!({"features":[]})).is_err());
    }
    #[test]
    fn cached_budget_is_bounded_and_resets() {
        let now = Instant::now();
        let mut limits = Limits {
            second: now,
            count: 0,
            day: 10,
            daily: 0,
            auth_blocked: [None; 2],
            quota_blocked: None,
            cache: HashMap::new(),
        };
        for _ in 0..4 {
            assert!(admit(&mut limits, now, 10, 5, 1));
        }
        assert!(!admit(&mut limits, now, 10, 5, 1));
        assert!(admit(&mut limits, now + Duration::from_secs(1), 10, 5, 1));
        assert!(!admit(&mut limits, now + Duration::from_secs(2), 10, 5, 1));
        assert!(admit(&mut limits, now + Duration::from_secs(2), 11, 5, 1));
    }
    #[test]
    fn auth_failure_blocks_only_failed_key_but_quota_blocks_both() {
        let now = Instant::now();
        let mut limits = Limits {
            second: now,
            count: 0,
            day: 10,
            daily: 0,
            auth_blocked: [None; 2],
            quota_blocked: None,
            cache: HashMap::new(),
        };
        record_failure(&mut limits, 0, 401, now);
        assert!(!eligible(&limits, 0, now));
        assert!(eligible(&limits, 1, now));
        assert!(quota_open(&limits, now));
        assert!(eligible(&limits, 0, now + Duration::from_secs(301)));
        record_failure(&mut limits, 1, 429, now);
        assert!(!quota_open(&limits, now));
        assert!(quota_open(&limits, now + Duration::from_secs(61)));
        assert!(admit(&mut limits, now, 10, 1, 1));
        assert!(!admit(&mut limits, now + Duration::from_secs(1), 10, 1, 1));
    }
}
