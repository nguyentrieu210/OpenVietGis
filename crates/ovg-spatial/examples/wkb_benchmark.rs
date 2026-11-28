// Synthetic CPU benchmark; no database, network or tenant data.
use geo_types::{Coord, Geometry, LineString, MultiLineString, MultiPolygon, Polygon};
use ovg_spatial::geometry::encode_wkb;
use std::hint::black_box;
use std::time::Instant;

fn main() {
    let vertices = 30_000;
    let line = LineString(
        (0..vertices)
            .map(|i| Coord {
                x: 105.0 + f64::from(i) / 1_000_000.0,
                y: 21.0 + f64::from(i % 17) / 100_000.0,
            })
            .collect(),
    );
    let multi = Geometry::MultiLineString(MultiLineString(vec![line.clone(); 3]));
    let polygon = Polygon::new(line, vec![]);
    let polys = Geometry::MultiPolygon(MultiPolygon(vec![polygon; 3]));
    for (name, geom) in [("multiline_90000", multi), ("multipolygon_90003", polys)] {
        let expected = encode_wkb(&geom, Some(4326)).unwrap();
        let mut samples = Vec::new();
        for i in 0..110 {
            let start = Instant::now();
            let bytes = encode_wkb(black_box(&geom), Some(4326)).unwrap();
            let elapsed = start.elapsed().as_secs_f64() * 1_000.0;
            assert_eq!(bytes, expected);
            black_box(bytes);
            if i >= 10 {
                samples.push(elapsed);
            }
        }
        samples.sort_by(f64::total_cmp);
        // Noncryptographic byte checksum, only for fixture comparison.
        let checksum = expected.iter().fold(0xcbf29ce484222325_u64, |h, b| {
            (h ^ u64::from(*b)).wrapping_mul(0x100000001b3)
        });
        println!(
            "{name},{},{checksum:016x},{:.6},{:.6}",
            expected.len(),
            samples[49],
            samples[94]
        );
    }
}
