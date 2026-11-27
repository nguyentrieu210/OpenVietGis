# ADR-015 Architecture Review — Rust-Native Platform Migration

- **Task**: Spec 064 — T005 (`FR-012`)
- **Reviewed ADR**: `docs/adr/ADR-015-rust-platform.md` (Status: `Proposed`, Date: `2026-11-25`)
- **Companion ADR**: `docs/adr/ADR-004-python-fastapi.md` (Status: `Accepted`, Date: `2026-11-17`)
- **Constitution Baseline**: `.specify/memory/constitution.md` (`v1.0.0`, Ratified `2026-11-17`)
- **Git Reference**: `064-rust-platform` (`babcdf9f00578ab336fb4404c90186fa55be9b51`)
- **Review Date**: 2026-11-25
- **Verdict**: **CONDITIONALLY READY FOR PHASE 1 PROTOTYPING — ADR-004 REMAINS `Accepted`**

---

## 1. Executive Summary

ADR-015 proposes migrating the OpenVietGIS core application backend from Python/FastAPI (`ADR-004`) to a Rust modular monolith (`Axum`/`Tokio`/`Tower`/`SQLx` + bounded `Rayon` workers + native `GEOS`/`GDAL`/`PROJ` adapters + authorized `Martin` tile gateway), while preserving PostgreSQL/PostGIS as the canonical data store and retaining React/MapLibre/Cesium, CAD bridges, and the PyQGIS connector.

Formal review against OpenVietGIS Constitution Principles I–VIII concludes that ADR-015 is **architecturally compliant** provided all staging, parity, security, CRS, and benchmark gates in Spec 064 (`T001`–`T022`) are satisfied before cutover. **No performance speedup is assumed prior to comparative measurement (`T003` vs `T020`).** ADR-004 MUST remain `Accepted` as the production behavioral oracle until all exit criteria in Section 5 are empirically proven and explicitly ratified by the project owner.

---

## 2. Constitution Compliance Matrix (Principles I–VIII)

| Principle | Constitutional Obligation | ADR-015 Assessment | Compliance Status | Mandatory Guardrails / Verification |
|---|---|---|---|---|
| **I. Platform-First, Module-Isolated** | Core provides broadly reusable spatial primitives only; domain verticals (e.g., house numbering) live behind stable SDK/contracts without Core special-casing them. | Proposed workspace separates `crates/ovg-core`, `ovg-storage`, `ovg-spatial`, `ovg-sync`, `ovg-processing`, and `crates/ovg-module-sdk` from `modules/house-numbering/rust/`. Clean installation works without domain modules. | **PASS (Gated)** | Verify `cargo metadata` dependency graph enforces zero reverse dependency from `ovg-*` core crates to `modules/*` (`T016`). |
| **II. Open Data and Interoperability** | User data remains portable via open formats (GeoJSON, GeoPackage, Shapefile, DXF, OGC) and documented APIs; module absence never locks user data. | PostgreSQL/PostGIS remains authoritative; GDAL/GEOS/PROJ native adapters preserve open format import/export and OGC interoperability; existing API contracts preserved. | **PASS (Gated)** | Differential format & round-trip export verification against GDAL `ogrinfo` and Python reference (`T010`, `T017`). |
| **III. Tenant Isolation Is a Security Boundary** | Explicit `organization_id` ownership; server + data-layer RLS enforcement across reads, edits, tiles, caches, streams, jobs, exports, and modules; fail closed. | SQLx runtime pool MUST connect exclusively as `openvietgis_runtime` (`NOSUPERUSER`, `NOBYPASSRLS`) with `SET LOCAL` transaction-scoped GUCs (`openvietgis.organization_id`, `openvietgis.actor_user_id`, `openvietgis.project_id`). Martin tile server NEVER exposed directly to public or raw tenant tables. | **PASS (High-Risk Gate)** | Dedicated negative cross-tenant test suite (`T007`, `T011`, `T019`) covering connection-pool reuse, savepoint rollback, tile ACL revocation, and SSE/WS teardown. |
| **IV. Spatial Correctness and Explicit CRS** | Source, storage, transform, and display CRS explicit and traceable; Vietnam-first (`VN-2000`: EPSG `4756`, `5899`, `3405`, `3406`) validated by control points. | Retains PostGIS (`PROJ 9.9.0`, `GEOS 3.15.0`) as authoritative spatial SQL/transform engine and native PROJ/GEOS bindings where server-side geometry ops run. GeoRust (`geo`) restricted to portable, differentially verified operations. | **PASS (High-Risk Gate)** | Golden VN-2000 control-point tolerances verified in `T010` before exposing any CRS transformation path; document exact PROJ pipeline strings and axis-order parity. |
| **V. Changes Are Versioned and Auditable** | Explicit attributable changesets, revision preconditions (`base_content_revision`, `base_schema_revision`), idempotency (`operation_id`), conflict detection, immutable audit records. | Retains existing transactional schema (`editing.changesets`, `editing.changeset_operations`, `audit.audit_events`), atomic outbox commit, and optimistic revision locking (`FOR UPDATE` + `409 Conflict`). | **PASS (Gated)** | Replay, idempotency, conflict, and undo/revert parity suite against live PostGIS (`T009`, `T013`, `T014`). |
| **VI. Capability-Driven Architecture** | Explicit layer/source/module capabilities (`read`, `edit`, `export`, etc.) and command convergence across UI/API/QGIS/MCP. | Preserves existing capability wire contract (`capabilities[]` on layer detail) and domain service layer behind Axum HTTP adapter; HTTP stays a thin adapter (same discipline as ADR-004). | **PASS** | Parity check on layer capability flags and error envelopes (`T002`, `T008`). |
| **VII. Web-Native, Engine-Agnostic** | Replaceable engines behind stable contracts (rendering, tiles, editing, identity, object storage, queueing, processing); avoid unnecessary abstraction. | Retains MapLibre/Cesium/React/PyQGIS/PostGIS; keeps tile delivery behind authorized gateway contract; avoids premature microservices or custom GPU renderer rewrite. | **PASS** | Keep trait boundaries only at real substitution points (`Storage`, `TileGateway`, `ProcessingExecutor`, `ModuleHost`). |
| **VIII. Workflow Value Before Feature Count** | Prioritize complete reliable workflows over feature parity; freeze feature expansion during rewrite (`FR-014`). | ADR-015 explicitly prefers staged replacement of proven workflows over wholesale rewrite or unmeasured engine additions; `FR-014` freezes feature expansion. | **PASS** | Reject any scope creep in `T006`–`T022`; optional engines strictly gated below. |

### Architectural Boundaries & Quality Gates Check
- **Short transactions & revision semantics**: Preserved; long-lived editing uses revision preconditions and `editing.changesets` rather than open DB transactions.
- **Bounded processing outside request path**: Enforced via bounded `Rayon` pool + Tokio semaphore/concurrency limits and explicit timeouts (`FR-008`), preventing Tokio async worker starvation.
- **No production secrets in repo**: Enforced via `gitleaks` and `tools/check_secret_surfaces.py`; Rust `secrecy` / `SecretString` equivalents required for config loading.
- **Additive migrations & rollback**: Enforced (`FR-011`); no destructive DDL before `T021` staging restore + rollback rehearsal passes.

---

## 3. Pinned Rust Toolchain & Build Environment Governance

### Pinned Toolchain
- **Rust Version**: **`1.85.1`** (stable)
- **Cached Local Build Image**: `rust:1.85.1-slim-bookworm` (verified on VPS1 Docker cache: Image ID `8573a005bfcb`)
- **Required Toolchain File** (to be committed in `T006` as `rust-toolchain.toml`):
  ```toml
  [toolchain]
  channel = "1.85.1"
  components = ["rustfmt", "clippy"]
  profile = "minimal"
  ```
- **Rust Edition**: `2024` is supported starting in Rust `1.85.0`, or `2021` may be chosen if any pinned crate macro requires it; workspace `edition` and `rust-version = "1.85.1"` MUST be explicitly set in root `Cargo.toml`.
- **Reproducible Builds**: All CI and release builds MUST pass `--locked` (`cargo build --release --locked`, `cargo test --locked`) with committed `Cargo.lock`.

### Host Capacity & Build Isolation Constraint
Observed VPS1 hardware (`2026-11-25`):
- **CPU**: 2 vCPU (`Intel(R) Xeon(R) Platinum 8171M @ 2.60GHz`)
- **RAM**: `3.8 GiB` total (`~1.7 GiB` available under live load), `4.0 GiB` swap (`~3.4 GiB` currently used)
- **Constraint**: Heavyweight Rust workspace compilation (`cargo build --release` with LTO + native `gdal-sys`/`geos-sys`) MUST NOT run directly on the live VPS1 production host while disk/swap are constrained. Builds MUST execute on an isolated CI runner or dedicated build container with bounded `CARGO_BUILD_JOBS=2` (or off-host CI producing commit-pinned OCI artifacts), and runtime targets MUST satisfy `SC-005` (`<= 1 GiB` aggregate app RSS on isolated 4 GiB profile with zero sustained swap growth).

---

## 4. Optional Engines — Gated Adoption Criteria

Per `FR-012`, `T005`, `T018`, and Constitution Principle VIII, **NONE** of the optional engines below are approved for Phase 1–3 implementation. Each is **GATED (Not Adopted)** and may only be integrated in `T018` or post-cutover after passing ALL listed promotion gates.

| Optional Engine | Proposed Use Case | Current Status | Mandatory Promotion Gates Before Adoption |
|---|---|---|---|
| **GeoArrow / GeoParquet** (`geoarrow`, `parquet`, `arrow`) | Columnar bulk spatial transfer, high-volume analytical export, zero-copy browser/worker interchange | **GATED (Not Adopted)** | 1. Measured >= 2x throughput or >= 40% peak RSS reduction vs PostGIS + GeoJSON/GeoPackage on >= 100k-feature benchmark.<br>2. Bit-exact CRS metadata (`projjson` / EPSG) and geometry round-trip fidelity.<br>3. Binary size and compile-time impact accepted; aggregate app RSS stays <= 1 GiB. |
| **SedonaDB / DataFusion** (`datafusion`, `sedona-*`) | Out-of-core analytical spatial SQL / federated GeoParquet queries | **GATED (Not Adopted)** | 1. Proven workload that PostGIS cannot meet within `SC-003`/`SC-004` budgets.<br>2. Tenant isolation proof showing zero cross-tenant cache/catalog leakage.<br>3. Memory governor enforces strict spill/cap within 4 GiB isolated host profile.<br>4. License/NOTICE audit complete (Apache-2.0) and crate maturity verified. |
| **Wasmtime** (`wasmtime`) | Server-side sandboxing of untrusted third-party marketplace plugins | **GATED (Not Adopted)** | 1. Required ONLY when onboarding untrusted third-party plugins (`E14`); official modules (e.g., house numbering) use trusted Rust SDK first (`T016`).<br>2. Fuel metering, epoch interruption, linear memory caps (<= 64 MiB/instance), and zero ambient filesystem/network/DB access verified by adversarial escape tests.<br>3. Explicit security sign-off on hostcall ABI. |
| **wgpu** (`wgpu`) | Custom GPU vector/3D/terrain rendering (desktop or headless server tile rasterization) | **GATED (Discovery Only — Not Adopted)** | 1. MapLibre GL JS + CesiumJS remain canonical web renderers.<br>2. No headless GPU hardware exists on VPS1 (`2 vCPU`, no discrete GPU).<br>3. Separate ADR required before any production commitment; out of scope for Spec 064 release. |
| **RustGIS / GeoLibre** (external community crates/projects) | Potential reuse of Rust GIS primitives or tile/render components | **GATED (Investigation Only — Not Adopted)** | 1. Inspect exact upstream repository, pinned release version, SPDX license compatibility with AGPL-3.0, `NOTICE`, transitive dependency tree, and active maintenance.<br>2. Zero source copying; consume only as audited, version-pinned dependencies.<br>3. Differential correctness + benchmark proof vs PostGIS/GeoRust baseline. |
| **Client-side WASM Kernels** (`crates/ovg-wasm` via `wasm-bindgen`) | Portable client geometry validation, snapping, or lightweight CRS/COGO helpers | **GATED (Optional in T018)** | 1. Pure-Rust deterministic kernels only (never assume browser GDAL/GEOS/PROJ equivalence).<br>2. Differential tests confirm <= 1e-9 coordinate parity with server PostGIS/GEOS.<br>3. Bundle size budget <= 250 KiB brotli-compressed. |

---

## 5. Readiness Notes for ADR-004 -> ADR-015 Transition

`docs/adr/ADR-004-python-fastapi.md` (**Status: `Accepted`**) MUST NOT be superseded during Spec 064 development. The Python/FastAPI backend remains the deployed production system and behavioral parity oracle until **every** transition gate below is closed with committed evidence:

1. **Scope & Parity Closure (`T001`, `T002`, `SC-001`)**:
   - 100% of inventoried routes, error codes, auth/session/CSRF semantics, and supported workflows (CAD, 3D, terrain, layouts, offline, chat/tasks, MCP, QGIS, house numbering) pass differential parity tests or hold an explicit owner-approved scope exception.
2. **Security & Correctness Closure (`T007`, `T010`, `T019`, `SC-002`)**:
   - Zero cross-tenant RLS leaks, zero tile cache ACL bypasses, zero unresolved Critical/High CVEs or `cargo-audit` advisories, and VN-2000 control-point tolerances met for every exposed CRS edge (`4326`, `3857`, `4756`, `5899`, and `3405`/`3406` if activated).
3. **Comparative Performance & Resource Proof (`T003`, `T020`, `SC-003`, `SC-004`, `SC-005`)**:
   - Same machine, same PostGIS database, same seeded workload (`tools/benchmark/rust-platform/benchmark_runner.py`):
     - At 20 concurrent clients: Rust viewport p95 `<= 70%` of Python baseline (`baseline-benchmark.json`), feature API p95 `<= 100%` of Python baseline, and error rate `<=` Python baseline.
     - Aggregate Rust app RSS `<= 1 GiB` on isolated 4 GiB profile with no sustained swap growth or OOM during soak test.
4. **Operational & Rollback Proof (`T004`, `T021`, `SC-006`)**:
   - Staging backup restore + forward migration + live writes + rollback to Python backend passes with verified row counts, deterministic content hashes, and zero lost acknowledged edits.
5. **Formal ADR Governance Action (`T022`, `T023`)**:
   - Upon owner sign-off on the exact release commit SHA:
     - Update `docs/adr/ADR-015-rust-platform.md` status from `Proposed` -> `Accepted` (citing evidence SHA and benchmark/rehearsal artifacts).
     - Update `docs/adr/ADR-004-python-fastapi.md` status from `Accepted` -> `Superseded by ADR-015` (retaining rollback artifacts until `T023` post-observation retirement approval).