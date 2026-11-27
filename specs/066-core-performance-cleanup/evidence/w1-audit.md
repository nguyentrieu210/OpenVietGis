# W1 audit closure
Pinned candidate for inventory/baseline: 70a038c29d7c4a3f5c5dd202a2f55669b97c1c90.
Baseline code parent: main 5bfc83826161acd83b98a925e85690cebde53ae1; the baseline browser build is spec-only d716ba7..., whose application code equals that main parent.

## Dependency / ownership
- full-inventory.json: 2,370 tracked paths with byte size, owner bucket, static relative JS/TS import edges/reverse counts and every Cargo/pnpm/Python manifest. Zero reverse-import count is explicitly not a deletion authorization.
- dependency-inventory.json: literal Rust embeds remain a separate guard; 27 Python files are embedded by Rust and therefore retained.
- Active 065 was reconciled as merge parent 1efc76f...; its QGIS UX remains evidence-gated and is not claimed accepted here.
- ADR-004 remains Accepted. ADR-015 remains Proposed. Specs 062 and 064 remain retirement prerequisites. Therefore old Web/Python/MCP runtime source cannot be removed in W2 yet.

## Contracts / runtime
- contract-inventory.json: 59 literal Rust HTTP method/path keys, 65 Python keys, 29 common, 30 Rust-only, 36 Python-only; 39 Rust MCP tools, 8 legacy Python MCP functions, 55 migrations and 521 test files. Prefix/dynamic router limitations are recorded in the file.
- runtime-retention.json: worktrees/images/containers/volumes and /opt candidates are classified. Live/rollback/data/active worktrees are protected. No deletion is authorized by reclaimable size alone.
- GitHub repository rulesets endpoint returned an empty array. The branch-protection endpoint returned HTTP 403 to the GitHub App even though repository metadata reports admin permission. Required-check configuration is therefore unobservable, not assumed absent; T004 stays open.

## Baseline
- Browser: 30 independent synthetic-loopback shell samples and 30 initialized CAD/3D transition samples; five raw cold-GIS engine samples. Baseline→candidate p95: cold shell 3077.0→2283.3 ms, warm shell 3530.9→2655.7, GIS→CAD 1750.9→1533.0, CAD→GIS 1517.5→1221.0, GIS→3D 3275.5→3108.4. No failures. Mode-transition 300 ms target is not met.
- HTTP: isolated PostGIS benchmark, same seeded DB/payloads and real Python/Rust servers. At concurrency 20 Rust/Python p95 ratio is 0.395 for attribute 1k and 0.524 for 30k bbox, response parity true and errors 0%.
- Build: cold Rust release 11m38s; exact warm rebuild 0.93s Cargo / 1.908s whole Docker command; binary 21 MiB. This validates caching value but is not a GHA build result.
- Bundle/offline/build-size, PostGIS SQL, WKB and render-diff baselines remain in prior evidence files.
