# Agent 04 — Release Provenance, Deploy & Rollback Audit (P0/D)

**Baseline audited:** `main@fc8b8de` · **Date:** 2026-11-23

## 1. Observed Live Production Topology (VPS Milion `103.77.243.154`)
- **Canonical public domain:** `https://openvietgis.cloud`
- **Frontend release mechanism:** Atomic symlink `/var/www/openvietgis-hn019/rewrite` pointing to immutable build directories under `/var/www/openvietgis-releases/v2026.11.23-<SHA>` (verified anchor: `v2026.11.23-5baade9`; latest live before sync: `v2026.11.23-cdcd5ff`).
- **Backend API service:** `openvietgis-hn019-api.service` (port `8019`) pinned via systemd drop-in `/etc/systemd/system/openvietgis-hn019-api.service.d/90-ovg-release.conf` to immutable directories under `/opt/openvietgis-releases/v2026.11.23-<SHA>` (verified anchor: `v2026.11.23-5baade9`; current: `v2026.11.23-ae06f4b`).

## 2. Deploy Safety Gates
Before promoting any new SHA to live:
1. `ruff check apps/api tools tests modules` = 0 errors & `mypy apps/api/openvietgis_api` = 0 errors.
2. `check_scope.py`, `check_architecture.py`, `check_contracts.py`, `check_docs.py` = OK.
3. Unit/contract/architecture pytest + `apps/web-next/integration-hub-smoke.mjs` + `web-next tsc --noEmit` + `vite build --base=/rewrite/` PASS.
4. Backward-compatible DB migrations (`alembic upgrade head`) verified on isolated test DB prior to live upgrade.
5. Immutable release copy + `brand/` preservation + `chmod -R a+rX` + atomic symlink flip + live HTTP smoke (`/` + `/api/v1/health`).

## 3. Rollback Rehearsal Plan
- **Frontend instant rollback (<5s):** `ln -sfn /var/www/openvietgis-releases/v2026.11.23-5baade9 /var/www/openvietgis-hn019/rewrite` (no Nginx restart required).
- **Backend instant rollback (<10s):** Point `90-ovg-release.conf` back to `/opt/openvietgis-releases/v2026.11.23-5baade9` (or previous verified release), `systemctl daemon-reload && systemctl restart openvietgis-hn019-api.service`.
- **DB compatibility rule:** Migrations (`0039_r027_connector_bindings`) are additive (`CREATE TABLE`), so older backend releases remain backward-compatible without destructive `alembic downgrade`.

## 4. Explicit Unknowns / Monitoring Gaps
- Automated synthetic uptime probe + error-rate alerting for `openvietgis-hn019-api.service` is `NOT_VERIFIED` in checked-in infra config.
- Structure-only audits (`control_kit_audit.py`) verify provenance/asset alignment only and never substitute for functional E2E acceptance.
