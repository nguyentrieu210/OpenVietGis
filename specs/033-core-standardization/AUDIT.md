# Core standardization — implementation evidence
Baseline: d110b13 (Spec 032). Static and targeted runtime checks on isolated candidate only.

## Implemented
- Backend: moved official module runtime discovery from domains/extensions/runtime.py to adapters/http/official_modules.py, preserving router discovery and manifest validation. No transport framework imports remain in domain under current static scan.
- Frontend: web-next official module manifest/runtime glob discovery moved from LiveApp into official-module-registry.ts. The scanner now detects typed import.meta.glob patterns.
- Legacy: eight lazy workbench imports and two stylesheet imports confined to legacy-workbench-bridge.ts; workbench IDs/titles extracted to workbench-contract.ts so tool state does not depend on view component.
- Spatial: unchanged SnapMath/SurveyMath implementations relocated to spatial-workspace/src, exported via package subpaths. Original app paths are compatibility re-exports. Two tests added to the existing package suite.
- Guard: rejects direct JS/TS app-internal imports from reusable runtime package source, reverse legacy-to-canonical imports and bypass of the legacy bridge; domain AST rejects FastAPI/Starlette/SQLAlchemy/psycopg imports. Existing vertical isolation rules retained. Static literal checks do not prove dynamic import safety or complete dependency graph.

## Compatibility workbench matrix
All rows have source presence and a synthetic Chromium command-opening check. This does not prove full real API behavior.
| Group | Commands exercised | Implementation retained |
|---|---|---|
| Spatial catalog | LA | SpatialCatalogPage |
| Transfers | IMPORT | ImportExportPage |
| Editing | EDIT | EditingWorkbenchPage |
| Query | QUERY | QueryWorkbenchPage |
| Processing | BUFFER, CLIP, DISSOLVE | ProcessingWorkbenchPage |
| Cartography | PLOT | CartographyWorkbenchPage |
| Platform hub | DXF, WMS, REVIEW, OPS | PlatformHubPage |
| Offline | FIELD | OfflineFieldPage |

## Validation
- Architecture guard and fixtures: 11 tests passed, no detected repository violation.
- Backend targeted foundation/spatial and extension contracts: 42 tests passed; official module discovery: 3 passed.
- Spatial workspace: 38 tests passed; separate snap and survey smoke scripts passed.
- web-next typecheck and build passed; legacy web and spatial-workspace typechecks passed.
- Chromium core-toolkit-smoke: 13 workbench commands, measurement, layer visibility, mocked map save and create-feature flows passed. Mock refreshed to current app path, ACL endpoint, layer selector and save notification; network limited to loopback. No production database or credentials used.
- Targeted Ruff and docs navigation passed.

## Residual work
- Core architecture guard is static, not a security certification. Check dynamic runtime contracts, SDK coupling, authorization and transaction boundaries in later specs.
- Legacy bridge is a transitional dependency, not elimination of legacy frontend. Complete real API parity and move reusable workbenches incrementally, with isolated tests.
- Large LiveApp and CoreMapTools should be decomposed by use-case/state ownership, not split mechanically by file length.
- Full CI, database migration/restore, real Windows QGIS and independent review remain open under Spec 030/032. No live changes or stable claim.
