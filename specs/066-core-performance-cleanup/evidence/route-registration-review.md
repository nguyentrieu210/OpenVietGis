# Spec 066 T008 — source review of frozen API route registrations

Reviewed candidate: 45c3586a8b1dbc129c2ab8829ba0954f6e12afb7 (PR #82).
This is a source review, not a runtime parity certificate or an ADR acceptance.

| Frozen engine group | HTTP operations reviewed |
| --- | ---: |
| E01–E03 | 81 |
| E04–E08 | 48 |
| E09–E11 | 95 |
| E12 | 26 |
| E13 | 6 |
| E14 | 27 |
| E15 | 72 |
| E16–E17 | 0 (separate MCP/web gates) |
| E18 | 8 |
| Total HTTP operations | 363 |
| Additional WebSocket upgrade (presence) | 1 |

Source review found corresponding registrations for the previously reported
missing E01 OIDC/avatar/invitation, E09 raster, E10 CAD, E12 external sources
and adapter transfers, and E13 package operations. E07 now has binary format
adapters for GeoPackage and ZIP Shapefile. E15's retained/pending inventory
labels predate the current handlers in files.rs, route_import.rs, field.rs,
records.rs and other mounted module routers.

Review included the merged cartography/designer router, map_render viewport
and display-extent routes, professional/raster tile suffix parsing, nested
organization chat paths, managed desktop commands and collaboration activities
(in editing.rs). The baseline presence WebSocket is an HTTP GET upgrade route.
No missing registration was identified in this source review. Registration
alone does not prove auth, contract, format, or business-behavior parity.

The first exact-candidate Rust platform job reached Clippy but stopped on
single_char_add_str in the newly added inventory test. Correct the literal
and pin the expected inventory count to 364 so a truncated baseline cannot
silently qualify. At the time of review the E06/E08, E15, E16, E17, E18,
Rust MCP and QGIS jobs on the reviewed commit were successful; the complete
PostGIS suite and other checks were not finished. The fixed commit still
requires exact-candidate runtime tests. Preserve the independent oracle,
Python engine adapters and migrations; ADR-015 remains Proposed.

T008, T044–T048 remain open until their own evidence and reviews exist.
