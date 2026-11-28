# Initial implementation slice — 27 November 2026

Baseline branch commit 8f12fbf; main ancestor 5bfc838. Existing Rust/React engine boundaries and Guest GIS removal unchanged.

- SHELL-03: Ctrl/Cmd+K, accent-insensitive multiword discovery, exact command resolution; fuzzy discovery cannot implicitly execute a command.
- SHELL-04: fix scale preset stale input.
- DOCK-01: clamp dock dimensions against current viewport and reapply on resize.
- TABLE-02: display actual feature identifier and distinguish filtered page, loaded page and source count. Full-source filtering/sorting remains open.
- PROC-01: shared search normalization and disabled-command reason; favorites/history and provider acceptance remain open.

Validation: frontend TypeScript typecheck PASS; Vitest 55/55 and Node 27/27 PASS on Node 26.13.1. git diff --check PASS. Manifest inventory is not acceptance. Release gate intentionally blocks all rows without paired evidence.

No authenticated QGIS/OVG comparison, browser screenshot, PostGIS integration, or release promotion performed in this slice. No tasks are marked completed merely on the basis of implementation or unit tests.
