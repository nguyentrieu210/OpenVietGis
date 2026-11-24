# Baseline audit — 2026-11-24

Pinned source: 27f0b35af966b7a46e7b17929395d9bc01515e51. Static audit only; production unchanged and stable release blocked.

## Results
- 1,583 tracked files; 376 test/support file candidates (not individual executed tests).
- 267 HTTP route decorator occurrences; 44 MCP registration occurrences across server profiles (not distinct public tools); 2,645 lexical JSX control/event occurrences (not unique rendered buttons).
- 32 capability groups with matching source candidates: 22 P0 and 10 P1. All runtime acceptance statuses NOT_RUN in this audit. F21 land asset has no matching dedicated test candidate; review shared tests before concluding coverage is absent.
- No Python AST parsing errors. Five inventory unit tests passed, Ruff passed, documentation navigation passed, two generated inventories byte-identical. Exact path coverage and all 2,956 candidate line references validated against pinned tree.

See [summary](evidence/summary.json), [feature registry](evidence/registry.json), [all files](evidence/files.csv), [static surfaces](evidence/static-surfaces.csv). Reproduce using:
```sh
python3 tools/product_inventory.py --commit 27f0b35af966b7a46e7b17929395d9bc01515e51 --out /tmp/ovg-inventory
```

## Observed operational topology (read-only snapshot)
- GitHub main 27f0b35; /opt/openvietgis checked out 96897a2 on a documentation branch, five commits behind main at observation.
- API service working directory /opt/openvietgis; public MCP service /opt/openvietgis-releases/v2026.11.23-5baade9/apps/mcp.
- Public rewrite symlink targets /var/www/openvietgis-releases/v2026.11.24-5d7c3b9-qgis-v1. Directory name is not proof of actual running source SHA. API process loaded-code SHA not established.
- Both apps/web and apps/web-next are tracked. Main Playwright config starts apps/web; web-next requires explicit coverage, not inference from legacy E2E.
- At initial observation 36 local branches, 27 worktrees; audit added an isolated worktree. Review ownership and unmerged commits before cleanup.

## Remediation backlog (proposed)
| ID | Priority | Owner | Action and exit condition |
|---|---|---|---|
| B01 | P0 | Release | Spec 032: inventory live artifacts, manifests, migrations and worktrees; reconcile exact source pins; choose one candidate; no destructive cleanup |
| B02 | P0 | Architecture | Map legacy/web-next capability parity and Core dependency boundaries; one public contract without dropping features |
| B03 | P0 | QA | Add actual web-next browser acceptance on isolated PostGIS/OIDC, including login, import, edit, reopen, export |
| B04 | P0 | Security | Tenant/ACL and revocation verification across API, SDK, MCP, QGIS and derived outputs |
| B05 | P0 | Spatial | CRS, data identity, changeset, topology and format roundtrip oracle tests |
| B06 | P0 | Frontend | GIS/CAD/3D mode switching, actual WebGL, save/reopen, tool visibility and dirty-state tests |
| B07 | P0 | Integration | Real Windows QGIS OAuth, bidirectional sync, conflict/reconnect/revoke; test offline recovery |
| B08 | P0 | Release | Exact candidate CI, migration, independent restore/rollback, approved performance profile and security review |
| B09 | P1 | Extensions | Dedicated land asset acceptance, marketplace lifecycle and house numbering fixture validation |
| B10 | P1 | QA | Rendered control inventory by role, viewport and mode; link each actionable control to API/command and evidence |

## Evidence policy
SOURCE_PRESENT means a matching source path only; NOT_RUN is not FAIL. Candidate tests may be helpers or indirect matches. Static route expressions may not reflect mounted prefixes; MCP registrations span profiles; lexical UI scan may miss dynamic/custom controls or include duplicates. Runtime route listing, tool discovery and browser walkthroughs on isolated fixtures are required before Integrated/Tested/Verified/Stable can be asserted. Historical evidence remains valid only within its own scope and commit; Spec 030 requires exact-candidate acceptance. No production data used; no feature deletion, merge, push or deployment.
