# SPEC 070 — Unified Geospatial Search, Places & Navigation
Parent roadmap: R004 workspace, R007 query, R010 external sources; R005/R008 for export/network analysis.
Status: implementation candidate, not accepted.
## User stories
- Find OSM addresses, places and POIs in Vietnamese, with or without diacritics; suggest while typing, bias to current viewport, identify source/accuracy, fly to point or extent in 2D/3D.
- Search coordinates with explicit CRS, project features and authorized module providers without leaking tenant data. Core must not depend on house numbering.
- Reverse geocode clicked coordinates; copy/save places; route from/to chosen locations with waypoints, vehicle profiles, alternatives, steps, distance/duration and route preview.
- Export route and places to GIS; offer matrix, isochrone and route optimization and MCP tools.
## Contracts
Authenticated API at /api/v1; fixed operator-configured Photon/Nominatim/Valhalla endpoints, no user-controlled upstream URL; bounded inputs, responses, timeouts and concurrency; fail closed if absent. EPSG:4326 longitude/latitude for geographic requests. No public OSM autocomplete service. Attribute © OpenStreetMap contributors; preserve provenance; no guaranteed street-number coverage or live traffic.
## Acceptance
Provider mock and real Vietnam data tests; no cross-tenant access; malformed inputs rejected; provider outages preserve internal locator; camera and route overlay tests; accessibility and keyboard tests; P95 suggest 300ms, geocode/reverse 500ms, route 2s under documented workload. Test import/update/rollback and attribution before live rollout.
