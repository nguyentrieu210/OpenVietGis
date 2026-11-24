# Spatial contract findings and targeted implementation
Baseline a9ee80d (Spec 033). No live changes.

## Canonical data contract
| Concern | Contract |
|---|---|
| Identity | organization/project/source/feature ID; layers may reference the same source |
| Native storage | Preserve stored CRS, coordinate order XY, native geometry and schema |
| Display | Explicit response CRS; display geometry is not a native edit input |
| Revision | Source content and schema hashes; separate integer layer/config revision |
| Edits | Authorized changeset, stable operation ID and exact current pins |
| QGIS bootstrap | Same layer/source, geometry family, XY dimensions, CRS, fields, content/schema and permission epoch across pages; unique stable IDs |
| Unknown input | Fail closed, no implicit coordinate assignment or destructive migration |

## Supported CRS definitions (unchanged)
4326 WGS84 geographic; 3857 Web Mercator; 4756 VN-2000 geographic; 5899 VN-2000 TM-3 107-45 are existing readable profiles, subject to validation and authorization. 3405/3406 VN-2000 UTM definitions remain definition-only. Readability does not prove every transformation or locality is supported. Existing directed transform and control-area restrictions remain in force; no new support for regional central meridians.

## Implemented and verified
- Domain CRS code sets and Mercator bound now derive from immutable CRS registry rather than duplicate literals. A cross-registry test checks coverage of supported export WKT definitions.
- Managed snapshot loader now validates page coherence and progress before QGIS materialization. Reject duplicate/invalid IDs, missing/unsupported CRS, unsupported dimensions/family, malformed revision, reserved/duplicate field name, cursor offset errors and mismatching permission epoch. Tests include projected coordinates preserved unchanged and inconsistent page rejection.
- QGIS decode failure for nonempty geometry now raises rather than silently returning empty geometry. Native QGIS runtime not available for acceptance; do not claim preservation of every geometry/attribute through QGIS conversion.
- 241 targeted Python tests passed (spatial, transfer, editing, connector, architecture); architecture, targeted Ruff, docs and web-next production build passed. Rebuilt QGIS 1.0.0 ZIP SHA256 df212611956f1cfb7c323904e430f6e7d4e2f525183a9440524b6be80ca07415; built frontend ZIP byte matches. Experimental release state unchanged.

## Remaining risks
- Multiple statements in backend bootstrap need exact transaction-consistent snapshot evidence under concurrent mutations, not just client inter-page checks.
- Managed delta currently resolves source to one layer using LIMIT 1; multi-layer aliases and historical event revision vs latest geometry require explicit consistency semantics and testing.
- Existing WKT/PRJ parser recognizes authority labels and limited parameters; conflicting authority/definition must be tested and validated in a dedicated import hardening slice. The new registry test checks code coverage, not semantic equivalence of WKT.
- Native QGIS decode, decimal/date conversion, CRS changes on local layers, geometry fidelity and actual reconnect/permission races require native tests. No projection scope expansion or database migration performed.
- Full exact-candidate CI, real PostGIS/OIDC, surveyed control points, Windows QGIS and independent release gates remain open.
