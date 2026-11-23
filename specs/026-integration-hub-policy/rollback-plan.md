# R026 Rollback Plan & Main-Branch Reconciliation Runbook

**Feature:** R026 — Integration Hub & Policy Gate (Slice 1 + Slice 2 Read-Only Profile Foundation)  
**Branch:** `feature/026-integration-hub-policy`  
**Base Commit:** `main@5baade9`  
**Reconciled Against:** `origin/main@2373a53`  
**Release State:** Candidate branch — **NOT deployed to production**.

---

## 1. Zero-Schema-Change Rollback Advantage

R026 Slice 1 and Slice 2 introduce **zero database migrations, zero DDL changes, zero new PostgreSQL roles/policies, and zero data mutations**:

- `apps/api/migrations/` is completely untouched (`git diff --name-only 5baade9 -- apps/api/migrations` is empty).
- All R026 HTTP endpoints (`GET .../integration-hub/catalog`, `GET .../integration-hub/adapters/{adapter_key}`) are strictly read-only.
- No persisted user, project, spatial, `gis_access`, or R010 `external-sources` state is created or altered by R026.

**Operational implication:** Rolling back R026 requires **no Alembic downgrade, no database restore, and no data backfill**. Reverting the application commit or redeploying the previous immutable API and `web-next` container images completely removes R026 with zero state drift.

---

## 2. Main-Branch Reconciliation Check (`origin/main`)

Verified non-destructively on 2026-11-23 without mutating git history:

```powershell
git log --oneline 5baade9..origin/main
# 2373a53 docs: link public OpenVietGIS demo and source in product direction
# db4784f docs: introduce OpenVietGIS community preview and live demo in README

git diff --stat 5baade9..origin/main
# README.md       | 34 ++++++++++++++++++++++++++++++++++
# docs/product.md |  5 +++++
# 2 files changed, 39 insertions(+)

git merge-tree --write-tree origin/main HEAD
# 1407a96a443e046f5b8315f26f86214d25b74e2f (exit code 0 — ZERO merge conflicts)
```

- The 2-commit delta on `origin/main` (`db4784f`, `2373a53`) modifies only `README.md` and `docs/product.md`.
- Neither file is touched by `feature/026-integration-hub-policy`.
- Merging or rebasing `feature/026-integration-hub-policy` onto `origin/main@2373a53` is conflict-free.

---

## 3. Rollback Procedures

### Option A — Fast Production Rollback via Previous Immutable Release Images (Preferred for Live Incidents)

Use this path if R026 has already been promoted to staging or production and must be reverted immediately (< 2 minutes RTO):

1. **Record incident metadata:** capture the active release SHA (`$CURRENT_SHA`), target previous verified release SHA (`$PREVIOUS_SHA`, e.g., pre-R026 `main` build), timestamp, and triggering symptom.
2. **Roll back API service (`openvietgis-api`):**
   - Redeploy the previous immutable API image pinned to `$PREVIOUS_SHA` (or revert deployment manifest tag to `$PREVIOUS_SHA` and trigger rolling restart).
   - Confirm zero Alembic changes are required (schema revision matches both `$CURRENT_SHA` and `$PREVIOUS_SHA`).
3. **Roll back frontend (`web-next`):**
   - Redeploy the previous immutable `web-next` static bundle / container image pinned to `$PREVIOUS_SHA` (which removes `IntegrationCatalog` from `LiveApp.tsx` and reverts `packages/client-sdk` bundle).
   - Purge CDN/reverse-proxy cache for `index.html` and static assets if applicable.
4. **Execute Section 4 Post-Rollback Verification Checklist.**

### Option B — Source-Level Git Revert on `main` (For Post-Merge Remediation)

Use this path after stabilizing production (or to cleanly back out a merged PR on `main`):

1. **If merged via squash/merge commit `$MERGE_SHA`:**
   ```bash
   git fetch origin
   git checkout -b revert/026-integration-hub-policy origin/main
   # For a merge commit (2 parents):
   git revert -m 1 <MERGE_SHA>
   # Or for a single squash commit:
   # git revert <SQUASH_SHA>
   ```
2. **Verify clean removal of all R026 surfaces:**
   - `apps/api/openvietgis_api/domains/integration_hub/` removed.
   - `apps/api/openvietgis_api/adapters/http/integration_hub.py` removed.
   - `apps/api/openvietgis_api/main.py` no longer imports or includes `integration_hub.router`.
   - `apps/api/openvietgis_api/adapters/http/gis_access_guard.py` removes `/integration-hub/catalog` and `/integration-hub/adapters/{adapter_key}` from `SAFE_GET` (all pre-existing `SAFE_GET` and `SAFE_POST` entries intact).
   - `apps/web-next/src/LiveApp.tsx` no longer imports or renders `<IntegrationCatalog />`.
   - `packages/client-sdk/src/index.ts` no longer re-exports `./integration-hub`.
3. **Run CI verification on the revert PR** (`validate:py`, `validate:ts`, `validate:arch`, `validate:docs`, `validate:integration`) and redeploy API + `web-next` from the reverted `main` SHA.

---

## 4. Post-Rollback Verification Checklist

Every item below MUST pass after executing Option A or Option B:

| # | Check | Expected Result |
| :--- | :--- | :--- |
| 1 | **API Health & Readiness** (`GET /api/v1/health`, `GET /api/v1/ready` or `/healthz`) | `200 OK`; database pool healthy; no startup/import errors in API logs. |
| 2 | **R026 Routes Removed** (`GET /api/v1/organizations/{org}/projects/{proj}/integration-hub/catalog` and `.../integration-hub/adapters/qgis` with valid auth) | `404 Not Found` (and paths absent from `/openapi.json`). |
| 3 | **R010 External Sources Unaffected** (`GET /api/v1/organizations/{org}/projects/{proj}/external-sources` with valid unscoped auth) | `200 OK`; existing allowlist, SSRF guard, and read-only preview/viewport behavior intact. |
| 4 | **R002 Tenancy, RLS & `gis_access` Intact** | Anonymous requests return `401`; cross-tenant requests fail closed (`403`/`404`); `gis_access` scoped members retain access to `SAFE_GET` (`/layers`, `/access/mine`, `/access/export/{layer_id}`) and are denied non-allowlisted paths (`403`). |
| 5 | **Database Schema Integrity** (`alembic current`) | Matches expected head revision with zero pending or drifted migrations. |
| 6 | **Frontend (`web-next`) Plugin Modal Clean** | Opening "Plugin của dự án" renders GIS Core, official extensions, and `ExternalPluginHub` without `IntegrationCatalog`, with zero console/page errors and zero failed `/integration-hub/*` network requests. |

---

## 5. Rollback Drill Status

- **Static rollback audit:** Completed on 2026-11-23 (zero migrations confirmed; isolated router, guard allowlist, SDK export, and UI mount points verified).
- **Live staging rollback drill:** **PENDING (GATE-04 / T009)** — must be executed in staging prior to production promotion and recorded with release/revert SHAs in `evidence.md`.
