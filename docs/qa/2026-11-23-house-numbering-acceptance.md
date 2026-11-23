# OpenVietGIS House Numbering – Acceptance review

**Date:** 2026-11-23 (Asia/Ho_Chi_Minh)
**Scope:** House Numbering module, Bình Hưng dataset and guarded production deployment
**Candidate preview:** https://openvietgis.cloud/rewrite/hn-batch-preview.html?live=1&v=acceptance-qr
**Official GIS:** https://openvietgis.cloud/app (unchanged)

## Acceptance matrix

| Workstream | Evidence | Status |
|---|---|---|
| Import lines and street/alley hierarchy | Isolated PostGIS route import/topology/segment integration; browser prefix/topology/recovery | PASS (test environment) |
| Survey address verification and grouping | Integrated address verification, suggestions, batch tests and browser smoke | PASS (test environment) |
| Corrected/verified building links | Assignment integrity checks, backend and browser confirmed-link scenarios | PASS (test environment) |
| Number allocation | Route-tree parity, chainage, duplicate, gaps, revision protection; PostGIS and browser smoke | PASS (test environment) |
| Approval, exported proposal packet | Project-authorized workflow and PDF/XLSX/plate tests, browser smoke | PASS (test environment) |
| QR links | Canonical `/app` URL with scoped parameters; same-tab navigation of survey item in test fixture, rejects unauthenticated PDF API | PASS (candidate preview) |
| GIS core regression | 5 browser scenarios: workspace-flex passed, core-plugin passed after updating a permissions fixture, workspace-access/core-toolkit/AI Copilot remain failing | BLOCKED FOR CANONICAL RELEASE |
| Legal issue of house numbers at Bình Hưng | Authoritative numbering rules, verified routes and eligible houses | BLOCKED (data/authority) |
| Release candidate to `/app` | Separate canonical release with AI consent features; broad regression gate incomplete | NOT PROMOTED |

## Automated evidence

- 57 unit tests PASS before the QR correction, and one additional QR URL unit test PASS (58 total).
- 22 PostGIS integration tests PASS on a disposable database (no writes to Bình Hưng).
- 16/16 House Numbering Playwright smoke scenarios PASS after an obsolete numbering-panel selector was corrected.
- QR parser and session persistence smoke PASS. Focused browser QR test PASS, including selecting the precise target survey feature on the preview.
- Unauthenticated GET of `/proposals` and `/packets/plates` returns HTTP 401.
- Main House Numbering API service remains active; preview assets deployed without modifying the official `/app` HTML.

## Live dataset (read-only)

| Metric | Count |
|---|---:|
| Buildings | 30,805 |
| Routes | 1,858 |
| Routes confirmed | 0 |
| Routes still draft | 1,858 |
| Buildings linked to a route | 5,703 |
| Linked buildings with `eligibility=unknown` | 5,703 |
| Linked buildings with `eligibility=eligible` | 0 |
| Buildings without a route link | 25,102 |
| Authoritative numbering rules | 0 |
| Active proposed numbers | 0 |
| Active approved numbers | 0 |
| Numbering proposals | 0 |
| Staged geometric reviews | 2 |
| Verified geometry-address reviews | 0 |

**Release gate:** Production issuance and public/legal publication must remain blocked. Never auto-confirm draft routes, auto-mark houses as eligible, invent a legal rule or auto-approve an administrative decision. Decision PDF is always explicitly marked as an **unsigned draft**.

## Remaining checks prior to canonical deployment

1. Resolve/refresh 3 open GIS Core regressions: AccessRules subject selector, map-configuration save confirmation, and AI Copilot integration behavior. Retest canonical and candidate independently. Do not replace `/app` until this gate passes.
2. Verify browser deep-link selection across an actual OIDC login (browser smoke presently emulates an authenticated session and separately checks session storage persistence).
3. Test 30k-house paging/performance and browser stability against a realistic, non-production-sized data fixture.
4. Have project officials confirm road/alley taxonomy, address evidence, building eligibility and authoritative rule, then run a limited witnessed pilot with written signoff.
5. Archive a backup of the official HTML release and check rollback/revalidation before replacing `/app`; maintain API authentication, audit logging and data separation.

## Repeatable verification

- Unit tests: `PYTHONPATH=apps/api:. apps/api/.venv/bin/python -m pytest -q modules/house_numbering/tests/ --ignore=modules/house_numbering/tests/integration`
- Integration: use `/tmp/ovg-hn-acceptance-20261123.sh` on VPS (creates disposable isolated Docker PostGIS).
- Browser: use `/tmp/ovg-hn-acceptance-browser.sh` against the accepted preview, plus `house-numbering-qr-navigation-smoke.mjs`.
- Typecheck: `cd apps/web-next && ./node_modules/.bin/tsc --noEmit`.

_This report is a technical test record, not a formal governmental number-issuance approval._
