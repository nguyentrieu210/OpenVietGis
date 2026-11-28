//! Bounded synthetic resolver load, not a HTTP/catalog or activation SLA.
use ovg_module_sdk::release_v2::{resolve_lock, Dependency, Pin};
use std::{
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc, Barrier,
    },
    time::Instant,
};
#[test]
fn resolver_1000_candidates_100_requests_four_workers() {
    let catalog: Vec<Pin> = (0..1000)
        .map(|i| Pin {
            key: format!("partner_fixture_{:03}", i / 10),
            version: format!("1.0.{}", i % 10),
            digest: format!("{:064x}", i + 1),
            dependencies: if i / 10 == 0 {
                vec![Dependency {
                    key: "partner_fixture_001".into(),
                    range: "^1.0".into(),
                }]
            } else if i / 10 == 1 {
                vec![Dependency {
                    key: "partner_fixture_002".into(),
                    range: "^1.0".into(),
                }]
            } else {
                vec![]
            },
        })
        .collect();
    let catalog = Arc::new(catalog);
    let next = Arc::new(AtomicUsize::new(0));
    let start = Arc::new(Barrier::new(4));
    let mut handles = vec![];
    for _ in 0..4 {
        let catalog = Arc::clone(&catalog);
        let next = Arc::clone(&next);
        let start = Arc::clone(&start);
        handles.push(std::thread::spawn(move || {
            start.wait();
            let mut times = vec![];
            loop {
                let n = next.fetch_add(1, Ordering::Relaxed);
                if n >= 100 {
                    break;
                }
                let t = Instant::now();
                let pins = resolve_lock("partner_fixture_000", "1.0.0", &catalog).unwrap();
                times.push(t.elapsed().as_micros());
                assert_eq!(pins.len(), 3);
                assert_eq!(pins[0].key, "partner_fixture_002");
                assert_eq!(pins[0].version, "1.0.9");
                assert_eq!(pins[1].version, "1.0.9");
                assert_eq!(pins[2].version, "1.0.0");
            }
            times
        }));
    }
    let mut times: Vec<u128> = handles
        .into_iter()
        .flat_map(|h| h.join().unwrap())
        .collect();
    times.sort_unstable();
    assert_eq!(times.len(), 100);
    println!("SPEC075_RESOLVER_LOAD candidates=1000 requests=100 workers=4 errors=0 p50_us={} p95_us={} p99_us={} max_us={}",times[49],times[94],times[98],times[99]);
    // Explicit rejection of an input beyond the documented solver limit.
    let mut oversized = (*catalog).clone();
    for i in 0..25 {
        oversized.push(Pin {
            key: format!("partner_extra_{i}"),
            version: "1.0.0".into(),
            digest: "a".repeat(64),
            dependencies: vec![],
        });
    }
    assert!(resolve_lock("partner_fixture_000", "1.0.0", &oversized).is_err());
}
