# Third-Party Dependencies, Licenses, NOTICE & Supply-Chain Policy

- **Task**: Spec 064 — T005 (`FR-012`)
- **Project License**: **GNU Affero General Public License v3.0 (`AGPL-3.0-only` / `AGPL-3.0-or-later` per repository `LICENSE`)**
- **Pinned Rust Toolchain**: `1.85.1` (cached image `rust:1.85.1-slim-bookworm`, ID `8573a005bfcb`)
- **Review Date**: 2026-11-25

---

## 1. License Compatibility Framework

OpenVietGIS is distributed under **GNU AGPL-3.0** (`LICENSE`). Under AGPL-3.0:
- **Compatible Permissive Licenses**: `MIT`, `Apache-2.0`, `BSD-2-Clause`, `BSD-3-Clause`, `ISC`, `Zlib`, `Unicode-3.0`, `CC0-1.0`, `Unlicense`, `BSL-1.0` — compatible with AGPL-3.0 when attribution and `NOTICE` requirements are preserved.
- **Compatible Weak Copyleft Licenses**: `LGPL-2.1-or-later`, `LGPL-3.0-or-later`, `MPL-2.0` — compatible with AGPL-3.0; LGPL native shared libraries (`libgeos_c`) MUST remain dynamically linked (or relinkable) with corresponding source availability disclosures.
- **Incompatible / Denied Licenses**: `GPL-2.0-only` (incompatible with GPLv3/AGPLv3), proprietary/non-commercial/source-available licenses (`BSL-1.1`, `SSPL-1.0`, `Elastic-2.0`, `Commons-Clause`), `CC-BY-NC-*`, and `WTFPL`/unlicensed crates without explicit SPDX headers.

---

## 2. Pinned Core Rust Crate Inventory

All crates below represent the pinned Phase 1–4 dependency set for the `apps/api-rust` + `crates/ovg-*` Cargo workspace. Exact patch versions MUST be locked in `Cargo.lock` upon `T006` initialization and audited before merge.

| Crate | Pinned Version Line | Role in OpenVietGIS Rust Core | SPDX License | AGPL-3.0 Compatible? | NOTICE / Attribution Notes |
|---|---|---|---|---|---|
| `axum` | `0.8.x` (`>=0.8.1`) | HTTP router, extractors, SSE/WS adapter | `MIT` | Yes | Retain copyright notice in `THIRD_PARTY_NOTICES.md`. |
| `tokio` | `1.x` (`>=1.43`) | Async runtime (`rt-multi-thread`, `macros`, `signal`, `sync`, `time`) | `MIT` | Yes | Retain copyright notice. |
| `tower` | `0.5.x` (`>=0.5.2`) | Service middleware, concurrency limits, timeouts, load shedding | `MIT` | Yes | Retain copyright notice. |
| `tower-http` | `0.6.x` (`>=0.6.2`) | CORS, request-id, trace, compression, body size limits | `MIT` | Yes | Retain copyright notice. |
| `sqlx` | `0.8.x` (`>=0.8.3`) | Async PostgreSQL/PostGIS driver, compile-time checked queries, pooling | `MIT OR Apache-2.0` | Yes | Elect `Apache-2.0` or `MIT`; include Apache-2.0 `NOTICE` if elected. |
| `serde` | `1.x` (`>=1.0.217`) | Serialization/deserialization framework | `MIT OR Apache-2.0` | Yes | Standard dual-license attribution. |
| `serde_json` | `1.x` (`>=1.0.138`) | GeoJSON, API wire JSON, manifest hashing | `MIT OR Apache-2.0` | Yes | Standard dual-license attribution. |
| `tracing` | `0.1.x` (`>=0.1.41`) | Structured instrumentation spans and events | `MIT` | Yes | Retain copyright notice. |
| `tracing-subscriber` | `0.3.x` (`>=0.3.19`) | JSON log formatting and `RUST_LOG` env filtering | `MIT` | Yes | Retain copyright notice. |
| `rayon` | `1.x` (`>=1.10`) | Bounded CPU thread pool (isolated from Tokio async workers) | `MIT OR Apache-2.0` | Yes | Standard dual-license attribution. |
| `geo` | `0.29.x` (`>=0.29.3`) | Portable pure-Rust geometry algorithms (differentially tested vs GEOS/PostGIS) | `MIT OR Apache-2.0` | Yes | Standard dual-license attribution. |
| `wkb` / `geozero` | `0.8.x` / `0.14.x` | Zero-copy WKB/EWKB/GeoJSON geometry decoding & encoding | `MIT OR Apache-2.0` | Yes | Verify exact PostGIS EWKB SRID flag parity. |
| `geos` (FFI to `libgeos_c`) | `9.x` / `10.x` (binding) + system `libgeos_c` (`3.11+` / `3.15` on PG) | Native topology validation, split/overlay/buffer parity with PostGIS GEOS | Crate: `MIT`; Native `libgeos`: **`LGPL-2.1-or-later`** | Yes (with LGPL obligations) | **MUST link dynamically** to system `libgeos_c`; disclose LGPL-2.1+ notice and corresponding source availability. See Section 3. |
| `gdal` (FFI to `libgdal`) | `0.17.x` (binding) + system `libgdal` (`3.6+`) | Native GeoPackage/Shapefile/DXF/Raster/COG import & export | Crate: `MIT`; Native `libgdal`: **`MIT` (X/MIT)** | Yes (audit enabled drivers!) | **Disable optional GPL/non-free GDAL drivers** (e.g. Poppler/MySQL/FileGDB) in container build; retain GDAL `LICENSE.TXT` / `PROVENANCE.TXT`. |
| `proj` (FFI to `libproj`) | `0.28.x` (binding) + system `libproj` (`9.x`) | Native CRS & VN-2000 pipeline verification (when server-side transform needed outside PostGIS) | Crate: `MIT OR Apache-2.0`; Native `libproj`: **`MIT`** (+ EPSG data terms) | Yes | Retain PROJ `COPYING` and EPSG Geodetic Parameter Dataset terms of use; `NETWORK_ENABLED=OFF` for deterministic offline transforms. |
| `uuid` | `1.x` (`>=1.12`) | UUIDv4/UUIDv5 entity & idempotency identifiers (`v4`, `v5`, `serde`) | `Apache-2.0 OR MIT` | Yes | Standard dual-license attribution. |
| `chrono` | `0.4.x` (`>=0.4.39`) | UTC timestamps, RFC 3339 wire serialization, session expiry | `MIT OR Apache-2.0` | Yes | Standard dual-license attribution. |
| `thiserror` | `2.x` (`>=2.0.11`) | Typed domain & HTTP error hierarchies | `MIT OR Apache-2.0` | Yes | Standard dual-license attribution. |
| `argon2` | `0.5.x` (`>=0.5.3`) | Memory-hard hashing for service credentials / secrets where KDF applies | `MIT OR Apache-2.0` | Yes | Constant-time verification required. |
| `hmac` + `sha2` + `subtle` | `0.12.x` / `0.10.x` / `2.6.x` | CSRF HMAC-SHA256, session token digests, manifest hashes, constant-time compare | `MIT OR Apache-2.0` (`subtle`: `BSD-3-Clause`) | Yes | Must match Python `csrf.py` and `sessions.py` byte-for-byte. |
| `jsonwebtoken` | `9.x` (`>=9.3`) | OIDC/JWT signature verification (RS256/ES256/EdDSA via JWKS) | `MIT` | Yes | Explicitly pin allowed algorithms; reject `alg: none` and HS* key-confusion. |
| `reqwest` | `0.12.x` (`>=0.12.12`) | Outbound OIDC discovery/JWKS and external OGC provider HTTP client (`rustls-tls`) | `MIT OR Apache-2.0` | Yes | Prefer `rustls-tls` over OpenSSL to reduce native TLS attack surface; enforce SSRF allowlist & timeouts. |
| `governor` | `0.8.x` (`>=0.8.1`) | Per-tenant / per-IP token-bucket rate limiting (GCRA) | `MIT` | Yes | Retain copyright notice. |
| `secrecy` | `0.10.x` | Zeroize-on-drop secret wrappers preventing accidental log leakage | `Apache-2.0 OR MIT` | Yes | Use for DB passwords, session/CSRF/bootstrap secrets. |
| `zip` (if pure-Rust archive inspection used) | `2.x` (`>=2.2`) | Bounded Shapefile/GeoPackage archive pre-inspection | `MIT` | Yes | Enforce entry count, path traversal, and expanded-byte limits before extraction. |

### External Runtime Services (Not Linked Crates)
| Component | Version Policy | License | Compatibility & Packaging Notes |
|---|---|---|---|
| **PostgreSQL + PostGIS** | `PostgreSQL 18.x` + `PostGIS 3.7.x` (observed on VPS1: `PG 18.7`, `PostGIS 3.7.0`, `GEOS 3.15.0`, `PROJ 9.9.0`) | PostgreSQL License (permissive) + PostGIS (`GPL-2.0-or-later`) | Separate process over TCP/Unix socket; no static linking with Rust binary. |
| **Martin Tile Server** (`ghcr.io/maplibre/martin`) | Pinned digest/minor (e.g. `v0.15.x` / `v0.16.x`) | `MIT OR Apache-2.0` | Separate container behind Rust authorization gateway; never publicly exposed. |

---

## 3. Special Native C/C++ GIS Library Obligations (GEOS / GDAL / PROJ)

1. **GEOS (`LGPL-2.1-or-later`)**:
   - **Linking Rule**: Dynamic linking (`libgeos_c.so`) MUST be used in Debian Bookworm container builds — NEVER enable static bundling features (`geos-sys/static`) in production images without providing relinkable object artifacts per LGPL-2.1 Section 6.
   - **Compliance**: Include LGPL-2.1 license text, copyright notice, and Debian package source correspondence pointer in container image `/usr/share/licenses/openvietgis/` and `THIRD_PARTY_NOTICES.md`.
   - **Preference**: Route primary spatial predicates and geometry validation through PostGIS SQL (`ST_IsValid`, `ST_Intersects`, `ST_Split`) where feasible to minimize in-process C-FFI crash exposure; isolate unavoidable in-process GEOS/GDAL parsing behind bounded worker pools with strict vertex/size limits.
2. **GDAL (`MIT` / X-style with heterogeneous drivers)**:
   - GDAL's core is MIT-licensed, but optional compiled-in drivers can pull in `GPL-2.0-only`, `GPL-3.0`, or proprietary SDKs.
   - **Build Rule**: Use Debian Bookworm `libgdal32` (or audited minimal build) and verify linked shared objects via `ldd $(which openvietgis-api)` + `dpkg-query` license check during `T019`; explicitly disable unused/risky drivers via `GDAL_SKIP` or driver allowlist (`GeoJSON`, `GPKG`, `ESRI Shapefile`, `DXF`, `GTiff`, `COG`).
3. **PROJ (`MIT` + EPSG Geodetic Parameter Dataset)**:
   - Include PROJ `COPYING` and EPSG attribution notice ("Contains data from the EPSG Geodetic Parameter Dataset").
   - Set `PROJ_NETWORK=OFF` in all containers so coordinate transforms are 100% deterministic and never perform unexpected outbound network calls.

---

## 4. NOTICE & Attribution Obligations

Before `T019` security/license sign-off and every release artifact build (`T022`):
1. Maintain a generated `THIRD_PARTY_NOTICES.md` (and container `/licenses/THIRD_PARTY_NOTICES.md`) using **`cargo-about`** (`cargo about generate about.hbs -o THIRD_PARTY_NOTICES.md`).
2. Preserve upstream `NOTICE` files from all `Apache-2.0` crates (including `sqlx`, `serde`, `rayon`, `geo`, `reqwest`, `uuid`, `chrono`, and any adopted `arrow`/`datafusion` crates).
3. Include native C library notices (`libgeos_c`, `libgdal`, `libproj`, `libc`/Debian base packages) in release SBOM (`CycloneDX` or `SPDX` JSON generated in CI).
4. Retain OpenVietGIS's root `LICENSE` (`AGPL-3.0`) unchanged and ensure network-service source offer notice remains intact.

---

## 5. `cargo-audit` and `cargo-deny` CI Enforcement Policy

Every PR touching `Cargo.toml`, `Cargo.lock`, or `crates/` MUST pass:
1. **`cargo audit --deny warnings`** (against RustSec Advisory Database):
   - **Blocking (CI Fail)**: Any `Critical` or `High` vulnerability advisory (`RUSTSEC-*`), or any yanked crate in `Cargo.lock`.
   - **Warning -> Triage Required (<= 14 days)**: `Medium`/`Low` or `unmaintained` advisories must either be upgraded or explicitly documented in `.cargo/audit.toml` with owner approval and expiration date.
2. **`cargo deny check all`** with committed `deny.toml` policy baseline:
   ```toml
   [graph]
   targets = [{ triple = "x86_64-unknown-linux-gnu" }]

   [advisories]
   version = 2
   db-path = "~/.cargo/advisory-db"
   yanked = "deny"
   unmaintained = "workspace"

   [licenses]
   version = 2
   confidence-threshold = 0.8
   allow = [
     "MIT",
     "Apache-2.0",
     "Apache-2.0 WITH LLVM-exception",
     "BSD-2-Clause",
     "BSD-3-Clause",
     "ISC",
     "Zlib",
     "Unicode-3.0",
     "CC0-1.0",
     "BSL-1.0",
     "MPL-2.0",
     "LGPL-2.1-or-later",
     "LGPL-3.0-or-later",
     "AGPL-3.0-only",
     "AGPL-3.0-or-later",
   ]
   # Explicitly deny GPL-2.0-only and non-OSI/source-available licenses
   private = { ignore = false }

   [bans]
   multiple-versions = "warn"
   wildcards = "deny"
   highlight = "all"
   deny = [
     { name = "openssl-sys", reason = "Prefer rustls to avoid native OpenSSL CVEs unless explicitly approved" },
   ]

   [sources]
   unknown-registry = "deny"
   unknown-git = "deny"
   allow-registry = ["https://github.com/rust-lang/crates.io-index"]
   allow-git = []
   ```
3. **Lockfile & Provenance Rules**:
   - Wildcard version specs (`*`) are **DENIED**.
   - Unpinned git dependencies are **DENIED** in release branches.
   - `Cargo.lock` MUST be committed and verified with `cargo check --locked`.