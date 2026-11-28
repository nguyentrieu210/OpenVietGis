# Wave 01: numbering and proposal safety

- Rust sequence generator rejects odd step for either parity; module unit tests passed 44/44.
- Proposal creation requires unique, active buildings in declared route/area descendant scope and the same rule as the selected proposed number.
- Submit/approve validate current number snapshot, rule, route, building state and eligibility under row locks; approval additionally requires confirmed routes, explicitly eligible buildings, nonblank authority reference and reviewer distinct from creator and submitter. Transition checks affected row count and excludes retired numbers.
- `cargo check --offline -p ovg-storage` passed using Rust 1.85.1. No production writes. These business guard changes intentionally strengthen previous behavior; database integration, concurrent cross-route conflicts, evidence verification and independent reviewer acceptance still require isolated PostGIS/API tests. No legal issuance or production approval claimed.
