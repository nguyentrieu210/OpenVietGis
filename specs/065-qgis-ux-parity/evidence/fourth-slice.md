# Fourth implementation slice
Base fe29f42c69e82d49123dcf00ae79789048c21011.

- SHELL-03: Ctrl+K locator supports explicit layer/feature/XY providers, Vietnamese accent folding, authorized active layers and loaded authorized features, bounded results, stale-search rejection. XY requires explicit longitude/latitude in EPSG:4326, within Web Mercator bounds; no implicit coordinate parsing. Choosing a result preserves unrelated layer visibility and does not cancel active edits.
- SHELL-04/05: scope display/recent CRS preferences by account/org/project; fix DMS rounding carry.
- Feature navigation handles polygons as well as points and lines.
- Validation: 62 Vitest + 32 Node tests, TypeScript, canonical build (exit 0), targeted synthetic Chromium smoke (exit 0). The existing canonical smoke runner includes the new smoke; complete runner not executed in this slice.
- Feature lookup does not search unloaded features; fixture API is intercepted, not actual Rust/PostGIS or QGIS Desktop acceptance. Matrix remains 12 partial, 59 unverified.
