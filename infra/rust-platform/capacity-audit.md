# Spec 064 VPS1 Resource Capacity Audit & Build/Staging Budget (T004)

- **Captured At**: `2026-11-25T16:45:39Z` (`2026-11-25 23:45:39 +0700`)
- **Host**: VPS1 `Milion` (`103.77.243.154`)
- **Kernel / OS**: `Linux 6.12.41+deb13-amd64` (Debian 13)
- **Requirements**: `FR-008`, `FR-011`, `SC-005`, `SC-006`

---

## 1. Measured Hardware & OS Capacity Snapshot

| Resource | Measured Total | Measured Used | Measured Available / Free | Utilization | Risk Level |
| --- | --- | --- | --- | ---: | --- |
| **CPU** | 2 vCPU (`Intel(R) Xeon(R) Platinum 8171M CPU @ 2.60GHz`, 1 core/socket × 2, no HT) | Background live services + Vite watcher | Shared across all live services | Moderate | High contention under parallel compiles |
| **RAM** | `3.8 GiB` (`4,112,334,848 B`) | `2.1 GiB` (`2,248,921,088 B`) + `1.9 GiB` buff/cache (`78 MiB` shared) | `194 MiB` free / **`1.7 GiB` available** (`1,863,413,760 B`) | 54.7% active / 45.3% available | **Tight** — no headroom for `rustc`/LLVM spikes |
| **Swap** | `4.0 GiB` (`/swapfile`, `4,294,963,200 B`) | **`3.4 GiB`** (`3,628,576,768 B`) | **`635 MiB`** (`666,386,432 B`) | **84.5%** | **CRITICAL** — near-exhausted swap |
| **Root Disk (`/dev/vda1`)** | `50 GiB` (`/`, `/opt`, `/var/lib/docker`) | **`43 GiB`** | **`3.7 GiB`** | **93%** | **CRITICAL** — insufficient for Rust `target/` or image builds |
| **`/tmp` (`tmpfs`)** | `2.0 GiB` | `1.6 GiB` | `338 MiB` | **83%** | **HIGH** — backed by RAM/swap; keep dumps small & cleaned |

### 1.1 Disk Footprint Breakdown

| Path / Subsystem | Measured Size | Reclaimable (Operator-Approved Only) | Notes |
| --- | ---: | ---: | --- |
| `/var/lib/docker` | `19 GiB` | ~`11.67 GiB` | Images `4.664 GB` (`3.031 GB` reclaimable); 74 Local Volumes `10.98 GB` (`8.287 GB` reclaimable, 60 unused); Build Cache `353.2 MB` (100% reclaimable); 13 exited containers |
| `/opt` | `15 GiB` | Operator review | 66 Git worktrees (~`9+ GiB` combined, e.g., 15+ worktrees at ~`396–415 MiB` each), `/opt/bolt-diy` (`1.3 GiB`), `/opt/openvietgis-releases` (`1.1 GiB`), `/opt/openvietgis-agent-worktrees` (`866 MiB`), `/opt/openvietgis` (`685 MiB`), `/opt/toolchains` (`469 MiB`) |
| `/root` | `3.2 GiB` | Operator review | Caches / tool data |
| `/tmp` | `1.6 GiB` (`tmpfs`) | Operator review | RAM/swap-backed tmpfs |
| `/var/log` | `294 MiB` | Low | Journal & service logs |

### 1.2 Toolchain Verification on VPS1

- `docker`: `26.1.5+dfsg1` (`/usr/bin/docker`), `docker compose`: `2.26.1-4`
- `cargo` / `rustc`: **Not installed** on VPS1 (`/opt/toolchains` contains only `node-v26.13.1-linux-x64` and `pwsh-7.6.10`).
- `psql` / `pg_dump` / `pg_restore`: Executed inside pinned PostGIS containers (`docker exec`) rather than host packages.

---

## 2. Running Containers & Live Services Inventory

### 2.1 Docker Containers (`docker ps` & `docker stats --no-stream`)

| Container | Image | Status | Loopback Port | CPU % | Mem Usage | Mem % | PIDs |
| --- | --- | --- | --- | ---: | ---: | ---: | ---: |
| `dev-keycloak-1` | `quay.io/keycloak/keycloak:26.8.2` | Up 2 days (healthy) | `127.0.0.1:18080->8080` | 0.39% | `369.6 MiB` | 9.42% | 52 |
| `hn019-preview-postgres` | `postgis/postgis@sha256:fe440a...` (18.7 / 3.7.0) | Up 3 days | `127.0.0.1:54340->5432` | 0.00% | `87.53 MiB` | 2.23% | 22 |
| `dev-postgres-test-1` | `postgis/postgis@sha256:fe440a...` (18.7 / 3.7.0) | Up 6 days (healthy) | `127.0.0.1:54330->5432` | 2.07% | `15.55 MiB` | 0.40% | 10 |
| `ovg-pro-test-7wdttm` | `0b8090246d94` (PostGIS) | Up 2 days | `127.0.0.1:54332->5432` | 0.00% | `8.13 MiB` | 0.21% | 9 |
| *(13 exited containers)* | `postgis/postgis:18-3.7`, `ovg-v1-profile-*`, etc. | Exited (2h–2d ago) | none | — | `0 MiB` | — | — |

### 2.2 Active Systemd Services & Top Memory Consumers

| Service / Process | Bind / Role | Measured RSS / `MemoryCurrent` |
| --- | --- | ---: |
| `openvietgis-web-dev` (`vite build --watch --base=/devui/`) | `/opt/openvietgis-agent-worktrees/dev058` | `546 MiB` (`559,136 KiB`) |
| `dev-keycloak-1` (Quarkus JVM, `-XX:MaxRAMPercentage=70`) | `127.0.0.1:18080` | `345 MiB` (`353,156 KiB`) |
| `openvietgis-hn019-api.service` (uvicorn master + 2 workers) | `127.0.0.1:8019` (`v2026.11.25-1aa8dcb-chat-rooms`) | `252 MiB` (`264,630,272 B`) |
| `dockerd` + `containerd` | Container runtime | `155 MiB` (`118,604 + 40,032 KiB`) |
| `mcp-codeeditor` (`node /opt/mcp-codeeditor/server.js`) | Dev tooling | `54 MiB` (`55,096 KiB`) |
| `openvietgis-api.service` (uvicorn 1 worker) | `127.0.0.1:8000` (`/opt/openvietgis`) | `36 MiB` (`36,392,960 B`) |
| `openvietgis-mcp` / `openvietgis-mcp-public` | `127.0.0.1:9188` | `31 MiB` (`31,648 KiB`) |
| `nginx.service` | Public reverse proxy (80/443) | `15 MiB` (`15,597,568 B`) |
| `openvietgis-web-ux.service` (Vite) | `127.0.0.1:5175` | `14 MiB` (`14,476 KiB`) |

---

## 3. Build-Host & Execution Constraints (Proof)

Running Rust compilation (`cargo build --release`, LTO, `sqlx`, `axum`, `tokio`, `geos`/`gdal`/`proj` sys crates, and Docker image builds) directly on VPS1 is **unsafe and prohibited**:

1. **Disk Exhaustion Proof**:
   - Available root disk: **`3.7 GiB`** on `/dev/vda1` (`93%` full).
   - Required disk for Rust toolchain (`~1.2 GiB`) + Cargo registry/git cache (`~0.8 GiB`) + workspace debug/release `target/` with spatial crates (`~3.5–6.0 GiB`) + OCI layer build cache (`~1.5 GiB`) = **`7.0–9.5 GiB`**.
   - Attempting a release build on VPS1 would hit `ENOSPC` (100% disk), risking PostgreSQL WAL write failures and crashing live `openvietgis-hn019-api` / `dev-postgres-test-1`.
2. **Memory & OOM Proof**:
   - Available RAM: **`1.7 GiB`**; free swap: **`635 MiB`** (total free headroom before OOM killer = `~2.3 GiB`).
   - `rustc` + LLVM codegen + linker (`lld`/`ld`) for a multi-crate Axum/SQLx/spatial release build peaks at **`2.5–4.5 GiB` RSS** even with `-j 1`, and `cargo test` parallel binary linking spikes higher.
   - Combined with live `dev-keycloak-1` (`MaxRAMPercentage=70`) and `vite --watch` (`546 MiB`), compilation on VPS1 would trigger swap thrashing or kernel OOM-kills of live PostgreSQL / API services.
3. **Mandatory Build Topology**:
   - All `cargo build --release`, `cargo clippy`, `cargo test` (unit), and multi-stage OCI image builds MUST run in **bounded CI (GitHub Actions)** or a dedicated isolated build host provisioned with **>= 4 vCPU, >= 8 GiB RAM, >= 40 GiB free NVMe/SSD**.
   - VPS1 (or an isolated 4 GiB staging VM) pulls ONLY pre-built, commit-pinned, stripped runtime OCI images (`OVG_API_RUST_IMAGE` pinned by Git SHA/digest).

---

## 4. Isolated 4 GiB Staging Memory Budget (`SC-005` Compliance)

`SC-005` requires that on an isolated 4 GiB profile, **aggregate application RSS <= 1 GiB (1024 MiB) excluding DB/OS**, with zero sustained swap growth or OOM. The staging stack in `infra/rust-platform/docker-compose.staging.yml` enforces hard cgroup limits (`mem_limit == memswap_limit` to forbid container swap growth):

| Container / Service | Category | Hard `mem_limit` | `memswap_limit` | CPU Quota | Key Memory / Concurrency Bounds |
| --- | --- | ---: | ---: | ---: | --- |
| `api-rust` | App (counted in SC-005) | **`512 MiB`** | `512 MiB` | `1.0` vCPU | `TOKIO_WORKER_THREADS=2`, `OVG_RAYON_THREADS=2`, `OVG_DB_MAX_CONNECTIONS=16`, bounded request body & query limits |
| `martin` | App (counted in SC-005) | **`256 MiB`** | `256 MiB` | `0.5` vCPU | `worker_processes: 2`, `pool_size: 8`, `cache_size_mb: 64`, internal network ONLY |
| `tile-acl-gateway` (Nginx) | App (counted in SC-005) | **`128 MiB`** | `128 MiB` | `0.25` vCPU | `worker_processes 1`, `worker_connections 512`, `proxy_cache_path` `max_size=64m`, fail-closed `auth_request` |
| **Aggregate App Total** | **SC-005 Target <= `1024 MiB`** | **`896 MiB`** | **`896 MiB`** | **`1.75` vCPU** | **`128 MiB` safety margin under SC-005 `1 GiB` ceiling; 0 swap permitted** |
| `postgres-staging` | DB (excluded per SC-005) | `1024 MiB` | `1024 MiB` | `1.0` vCPU | `shm_size: 256m`, `shared_buffers=256MB`, `work_mem=8MB`, `effective_cache_size=512MB`, `max_connections=64` |
| **Staging Stack Grand Total** | App + DB | **`1920 MiB`** | **`1920 MiB`** | — | Leaves `~2.1 GiB` for Linux OS & page cache on an isolated 4 GiB staging host |

**VPS1 Co-location Warning**: Because VPS1 currently has only `1.7 GiB` available RAM and `3.7 GiB` free disk while running live services, the full Docker Compose staging stack (`1.92 GiB` peak) MUST be deployed either on a dedicated isolated 4 GiB staging host OR on VPS1 **only after** operator-approved disk reclamation (`>= 12 GiB` free) and stopping non-essential dev watchers (`vite --watch` `546 MiB`).

---

## 5. Operator-Approved Disk Remediation Checklist (Not Executed Automatically)

Before running full staging soak tests (T020) on VPS1, operators may approve the following non-destructive cleanups (estimated recovery: **15–20 GiB**):

1. Review and remove 13 exited historical test containers (`ovg-v1-perf-*`, `ovg-v1-recovery-*`, `ovg-060-test`, `ovg-v1-profile-*`) and prune dangling/unused Docker volumes (`8.287 GiB` reclaimable) + unused images (`3.031 GiB` reclaimable) + build cache (`353 MB`).
2. Archive and prune merged historical Git worktrees (of 66 registered worktrees, >45 are clean, merged, historical branches consuming ~`8–10 GiB` under `/opt`).
3. Keep live containers (`dev-postgres-test-1`, `hn019-preview-postgres`, `dev-keycloak-1`), active release directory (`/opt/openvietgis-releases/v2026.11.25-1aa8dcb-chat-rooms`), `/opt/openvietgis`, `/opt/openvietgis-pre-rust-e2519e4`, and `/opt/openvietgis-rust-platform-spec` untouched.

