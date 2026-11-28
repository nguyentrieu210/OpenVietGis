# W0 qualification runbook — no PASS claimed yet

## Implemented source
- infra/desktop/wave0/protocol.rs: dependency-free logical URL grammar, UUID/revision/XYZ validation, rejection of encodings, query, fragments and traversal; adversarial ASCII mutation tests. Parser grants no authority.
- infra/desktop/wave0/tauri-spike: isolated Tauri host with a four-byte synthetic response only, exact origin allowlist, no project/filesystem/database access, no listening network socket; probe reports protocol fetch and WebGL2/WebGPU availability. This is not the application, renderer or production resource handler.
- .github/workflows/spec074-wave0.yml: parser test with Rust 1.85.1, cargo check on Windows/macOS/Linux and existing shared React build. Lockfile artifacts are exploratory, not committed production locks. Actual WebView launch and hardware tests are separate.

## Threat and ownership gates
Asset URL is not authorization. Production handler needs per-window/session/workspace/resource/revision/lease checks, response revalidation, exact origin and CSP, no wildcard CORS, bounded queues/resources, path grants and revocation. Probe's static origin check is insufficient for real data; no local provider linked. Unknown capability fails closed. Test invalid request, stale generation, logout/close race, cross-window/project, revocation, oversized request, decode bombs, symlinks and protected cache. No credentials, arbitrary SQL/shell/network, direct PostGIS access or raw path in frontend.
Signed package/update, native-library supply chain, notices/SBOM, OS secure storage and migration rollback are later release requirements; W0 records candidates and signing blockers, not acceptance.

## Run
From root: rustc --edition=2021 --test infra/desktop/wave0/protocol.rs -o protocol-test; run binary.
On each OS with Rust, WebView and system requirements: cd infra/desktop/wave0/tauri-spike; cargo generate-lockfile; cargo check --locked; cargo run --locked. Click Run probe; record JSON, OS, CPU/GPU, WebView version, exact SHA, origin and actual successful transport. Missing origin, failed custom URL or unavailable GPU is a failure to investigate, not a PASS. No broadening origin policy to conceal failure.
For shared UI: use root pinned Node/pnpm; pnpm install --frozen-lockfile; pnpm --filter @openvietgis/web-next build. Next W0 spike must actually load shared UI and exercise real MapLibre/Cesium fixtures in each WebView; synthetic HTML and a successful build alone are insufficient.
Record local artifact and runner logs. Freeze per-target lock/toolchain and dependency graph; select reference hardware, dataset generator parameters/hashes and performance scripts per performance contract.

## Outstanding before closing W0
T001 locked transitive graphs and latest integration head; T002 shared contract approval; T003 actual React/MapLibre/Cesium launch and GPU/transport on real OS; T004 dependency/license/security review and signing feasibility; T033 exact candidate provider/dependency and parity review; T034 reviewed platform URL mapping and scoped authority tests. No task checkbox is closed; no Windows/macOS/Linux package or live deployment.
