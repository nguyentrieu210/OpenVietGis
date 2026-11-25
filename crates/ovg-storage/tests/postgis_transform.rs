//! Live check of the server-side transform path against the recorded PostGIS
//! differential fixture. Runs only when `OVG_TEST_DATABASE_URL` names a PostGIS
//! database; the portable kernel is compared to the same fixture offline.

use ovg_core::crs::reference_transformation;
use ovg_storage::spatial_transform::apply_steps;
use serde_json::Value;
use sqlx::{Connection, PgConnection};

const FIXTURE: &str =
    include_str!("../../ovg-spatial/tests/fixtures/vn2000_postgis_differential.json");

fn xy(value: &Value) -> (f64, f64) {
    (value[0].as_f64().unwrap(), value[1].as_f64().unwrap())
}

#[tokio::test]
async fn fixed_pipelines_reproduce_recorded_postgis_output() {
    let Ok(url) = std::env::var("OVG_TEST_DATABASE_URL") else {
        eprintln!("skipped: OVG_TEST_DATABASE_URL is not set");
        return;
    };
    let mut conn = PgConnection::connect(&url).await.unwrap();
    let fixture: Value = serde_json::from_str(FIXTURE).unwrap();
    let points = fixture["points"].as_array().unwrap();
    let datum = reference_transformation(4756, 4326).unwrap();
    let inputs: Vec<_> = points.iter().map(|p| xy(&p["vn2000_lonlat"])).collect();
    let outputs = apply_steps(&mut conn, datum, &inputs).await.unwrap();
    for (point, (x, y)) in points.iter().zip(outputs) {
        let (ex, ey) = xy(&point["wgs84_pipeline_6960"]);
        assert!((x - ex).abs() < 1e-9 && (y - ey).abs() < 1e-9, "{}", point["id"]);
    }

    let tm3 = reference_transformation(4756, 5899).unwrap();
    let anchorage: Vec<_> = points.iter().filter(|p| p["epsg"] == 5899).collect();
    let inputs: Vec<_> = anchorage.iter().map(|p| xy(&p["vn2000_lonlat"])).collect();
    let outputs = apply_steps(&mut conn, tm3, &inputs).await.unwrap();
    for (point, (x, y)) in anchorage.iter().zip(outputs) {
        let (ex, ey) = xy(&point["projected"]);
        assert!((x - ex).hypot(y - ey) < 1e-6, "{}", point["id"]);
    }
}
