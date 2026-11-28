# Second implementation slice
Base 3a248f94638a1e65f8e938ab1b5927256b5c8474.

- SHELL-05 / DOCK-01 / DOCK-02: isolate personal dock layout, splitter, floating position, and shortcut toggle by account/org/project; remount across scope changes; no unsafe migration of unscoped legacy preferences. Keyboard float movement and viewport containment.
- TABLE-02: typed server-side single-field filtering and sorting using existing R007 Rust query endpoint, pinned content/schema and query fingerprint, verified response and separately authorized feature detail. The route excludes geometry-absent rows and limits sources to 50,000 objects and route offsets to 25,000. Multi-field filters, unrestricted source coverage and real browser acceptance remain open. Local view remains explicitly distinct.
- PROC-01: bounded project-scoped favorites and recently opened command IDs, intersected with current command registry and disabled state. Recent means opened, not successful processing job.
- Validation: TypeScript PASS; 58 Vitest and 30 Node tests PASS; git diff --check PASS. No QGIS desktop or authenticated Rust browser acceptance; no matrix row promoted to verified.
Canonical frontend build PASS.
