# T008 migration and operations audit — E10 CAD

Status: **Partial; T008 remains open.** This audit found no migration or runtime-permission mismatch for the E10 CAD/georeferencing slice. No migration, Rust migration runner, backup script, or deployment config was added or changed.

## Exact candidate and environment

- Tested source checkout: `agent/066-e10-cad` at `e250358e50e3965e678595d93834f22b6fb106dc` (the checkout SHA before this evidence-only commit).
- Rust candidate was built from that exact checkout with `cargo build --locked -p openvietgis-api-rust --bin openvietgis-api-rust`.
- Rust toolchain: `rustc 1.85.1 (4eb161250 2025-03-15)`, Cargo `1.85.1 (d73d2caf9 2024-12-31)`.
- Candidate binary SHA-256: `d69a065fd6a0d1900658b1f2780110732238c07fd478ac69a57964a6e507568b`.
- Rust E10 source file SHA-256: `apps/api-rust/src/cad_bridge.rs` = `5336718bdde53a4c9607ed42c813821217ea3d353ae2e927cadc4e4aa02d1f71`; `crates/ovg-storage/src/cad_documents.rs` = `29466239044149d05a7a200cc70dc53f5e199c6f7493139029eea948e446d5d7`.
- Python: 3.13.5; Alembic 1.20.1; SQLAlchemy 2.1.9; psycopg 3.3.9; FastAPI 0.163.1.
- Disposable database container: `ovg-t008-alembic-a53872bf`, image `postgis/postgis@sha256:fe440a27c6567aefa932e988962e5827bb2c29e4b97dac2ca5268f5eafe10262`; PostgreSQL 18.7, PostGIS 3.7.0; database `openvietgis_test` over loopback port 54331. The container had no data volume and was removed after the run; the port was released. The parity harness created and dropped its generated `openvietgis_bench_parity_test_*` databases; final count was zero. No production, staging, or shared test database was used.

## Audit results

| Gate | Result | Evidence |
|---|---|---|
| Alembic authority | PASS | `alembic heads` reports the single head `0046_map_display_config`, whose parent is `0045_postgis_bbox_leakproof`. Earlier parallel histories converge through the checked-in merge revisions. The Rust app/storage tree has no SQLx migration runner. |
| CAD schema and grants | PASS | Alembic `0015_cad_precision` creates `cad.conversions`; `0029_cad_documents` creates `cad.document_versions`. Under the initialized `openvietgis_runtime` role, both tables have FORCE RLS, SELECT and INSERT, and no UPDATE or DELETE. The role has `USAGE` on `cad` and EXECUTE on `gis_access.scoped(uuid,uuid)`. With no project GUC scope, runtime queries return zero rows. |
| Bootstrap | PASS | Fresh disposable database upgraded through the one Alembic head, `0046_map_display_config`. |
| Rollback and rebootstrap | PASS | `alembic downgrade base` left zero rows in `public.alembic_version` and removed `cad.document_versions`; `alembic upgrade head` restored the single `0046_map_display_config` row. Runtime role remained `NOSUPERUSER NOBYPASSRLS`. |
| E10 Rust queries and writes | PASS | `tests/rust-parity/e10_cad_dxf.py` passed 15 named CAD/georeferencing cases, including CRS rejection, import, georef draft/native preview, foreign-tenant denial, and stale revision/fingerprint/SHA rejection. `tests/rust-parity/e10_cad_documents.py` passed six cases for document listing, latest-per-document selection, version history/order, missing documents, and tenant denial. Both ran differential FastAPI-oracle/Rust checks against isolated databases on the disposable PostGIS instance. |
| Operations wiring | PASS (static) | `infra/rust-platform/docker-compose.hybrid.yml` runs a Python `migrate` service using the explicit migration-owner URL for Alembic and official-module migrations. Both API services depend on that service completing successfully. Rust startup does not own a second migration lane. |
| Backup / live safety | PASS (scope) | Backup/restore and rollback scripts were inspected and left unchanged. No backup/restore rehearsal or live/staging operation was run. |

## Commands

```sh
alembic -c apps/api/alembic.ini heads
alembic -c apps/api/alembic.ini upgrade head
alembic -c apps/api/alembic.ini downgrade base
alembic -c apps/api/alembic.ini upgrade head
OVG_PARITY_PG_ADMIN_URL=postgresql://openvietgis:openvietgis@127.0.0.1:54331/postgres \
  python tests/rust-parity/e10_cad_dxf.py \
  --candidate target/debug/openvietgis-api-rust
OVG_PARITY_PG_ADMIN_URL=postgresql://openvietgis:openvietgis@127.0.0.1:54331/postgres \
  python tests/rust-parity/e10_cad_documents.py \
  --candidate target/debug/openvietgis-api-rust
```

## Remaining T008 limits

No mismatch required a fix, so no Alembic revision was created. This audit covers E10 CAD document/georeferencing storage and route queries only. It does not close other 064 route groups, operations acceptance on a staging release, ADR-015, independent review, or the remaining T008 acceptance gates.
