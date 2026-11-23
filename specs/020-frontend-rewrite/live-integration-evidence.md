# 020 — Live integration, staging (2026-11-22)

## Entry points
- Demo: https://vps1.gpcds.site:2087/rewrite/
- Authenticated Studio: https://vps1.gpcds.site:2087/rewrite/?live=1
- Existing production 8443 untouched.
- Source: apps/web-next. Built with Node 26. Docker/API configuration unchanged.

## Implemented
- Keycloak via existing backend session and CSRF; no browser-stored credentials.
- Organization/project creation and switching, team membership read, logout.
- Real layer catalog, revision-pinned features transformed by API to EPSG:4326, MapLibre vector rendering, layer visibility, attribute table and selection.
- Geometry drag and numeric coordinate edit for editable EPSG:4326 point sources; revision-pinned idempotent changeset, receipt verification.
- Import asset upload, inspection/CRS resolution, conversion approvals, explicit confirmation and refresh; add layer from existing source.
- Official house numbering activation, trees for areas and street/alley, linked building selection, registry create forms, inspection and preview/apply logic; existing proposal/reconciliation/survey/document/export panels mounted in modal without replacing map.
- Read limit 100 layers / 500 features per layer, with warning if truncated. Not certified for large projects.
- No synthetic map data in authenticated mode. No production cutover.

## Verification
- TypeScript typecheck and Vite production build passed.
- Browser test performed sign-in, created a dedicated test org/project, imported GeoJSON, selected point, edited longitude, committed real changeset and checked updated coordinates.
- Reference fixture layers correctly reject editing.
- Full HN smoke blocked: staging API returns HTTP 500 on activation. Journal verified missing relation extensions.activation_events. Read-only inspection: staging migration URL points to openvietgis_test, alembic_version is 0019_extension_lifecycle; current source revision is 0019_extension_history. Existing schema includes some lifecycle columns; blindly applying/stamping migration is unsafe. No schema mutation made.
- Fresh offline pnpm lockfile validation blocked by missing package registry metadata (not bypassed); compile/build with existing installed dependencies passed.
- Registration / password recovery depend on IdP configuration. Invitation mail via Resend not integrated; no secret added to frontend or committed.
- Live smoke has CORE_ONLY=1 mode; full mode intentionally fails if HN activation cannot run.

## Next acceptance gate
Reconcile staging DB migration history with actual schema using verified backup and migration plan; then run HN browser test including route/building linking, numbering and proposal workflows. Implement registration and invitation delivery securely server-side. Complete responsive, permissions, full geometry editing and large-data testing before production cutover.
