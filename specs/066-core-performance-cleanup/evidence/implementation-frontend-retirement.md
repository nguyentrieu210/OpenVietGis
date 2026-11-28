# 066 implementation slice 17 — retire the legacy SPA and repair canonical OAuth entry

Parent candidate: 9edba91db65727f6f843af9ae2ccec43fdeb5206.

## T007 — old SPA retirement
- Removed the tracked apps/web application only after migrating every active consumer: pnpm workspace/lock importer, production web Docker dependency layer, E17 workflow path filters/source diff, Playwright main/house-numbering web-server cwd, architecture/scope gateways and product inventory.
- Root validate:ts, dev:web and build:web now select only @openvietgis/web-next. A fresh pnpm install --offline --frozen-lockfile succeeds with five workspace projects and no network download.
- Canonical frontend after deletion: typecheck PASS; 23 Vitest files / 108 tests PASS; 40 Node tests PASS; production build PASS and offline shell generated 10 static assets.
- Python architecture/scope/canonical-build checks PASS after retirement. Playwright config discovery lists 49 canonical E2E tests in 22 files; there is no active runtime/build reference to apps/web. The remaining literal legacy strings are negative architecture fixtures/inventory classification, not consumers.
- Offline Chromium suite PASS 8/8 after deletion, including CAD direct resume, authorization restriction, GIS lazy CAD/Cesium isolation, session-expiry cache purge, public entry isolation, service-worker upgrade preservation, special-entry isolation and durable offline draft/sync.
- The aggregate canonical smoke can exceed its historical watchdog while this VPS is under concurrent Rust/Finance/buildkit load; this slice does not convert a timing flake into a false PASS. The constituent web unit/build/offline gates above are exact and clean.

## Canonical OAuth/deep-link correction discovered by real E2E
- The migrated landing page attempted to embed Keycloak in an iframe. Browser evidence showed the frame became chrome-error://chromewebdata/ because the cross-origin identity provider disallows framing. Authentication now opens a user-initiated popup instead; AuthComplete already had opener postMessage support, so the same-origin callback verifies the application session before closing the popup.
- Internal protected paths such as /app/account are preserved through the OAuth return_to callback. Arbitrary external return_to query input is ignored; the generated callback remains same-origin.
- Reachable workbench SessionBoundary no longer redirects to the retired /login?return_to= route. On 401 it reloads the exact protected URL so canonical SessionEntry owns the unauthenticated state and popup login.
- Real OIDC browser test used an isolated Keycloak 26.8.2 container on 127.0.0.1:18101, isolated PostGIS 3.7.0 test DB on 127.0.0.1:54331, API 8026 and web 5189. No live Keycloak/database was modified. Auth-navigation PASS 4/4: modal/popup separation, /app/account deep-link preservation, hostile external return target rejection, logout plus protected-route denial.
- The initial diagnostic attempt against 18080 identified that port as the live loopback Keycloak and made no successful configuration change; all subsequent mutable identity-fixture work was isolated on 18101.

This closes Spec 066 T007 and Spec 062 T007/T009/T010. Spec 062 T008 and the broader Spec 066 final E2E/security convergence remain open.
