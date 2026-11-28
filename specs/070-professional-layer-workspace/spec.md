# SPEC 070 — Professional Layer & Label Workspace 2.0

Parent roadmap: R004 (map), R006 (editing), R007 (query), R009 (cartography), R011 (collaboration), R015 (release). Extends 056/065/069. User approval: 2026-11-28. Target environment: dev only.

## User stories
- US1: A GIS operator selects one or many layers, sees separate visibility, selection, edit, lock, snap, filter and label states; applies actions without opening modal dialogs.
- US2: A cartographer configures symbols and labels with live previews, reusable style recipes, scale rules, priorities, collision settings and explicit placement.
- US3: A surveyor pans/zooms a large polygon without duplicate labels, moves/pins/rotates a label independently of geometry and restores automatic placement.
- US4: A project owner organizes nested groups and render order independently, shares a revisioned configuration, switches themes and reopens the project without loss.
- US5: An operator using tiled large data receives the same supported visibility, selection, scale and style behavior as GeoJSON; unavailable attribute-dependent capabilities are explicit rather than silently wrong.

## Functional requirements
- E01: Unified typed layer state and policy, scoped personal preferences vs revisioned shared map configuration. Server authorization remains authoritative.
- E02: Compact discoverable layer rows, accessible column controls, counts/status, quick style/label controls, collapse/resize basemaps, keyboard focus and shortcuts.
- E03: Group context commands by intent and capabilities, with focus/escape and safe destructive actions.
- E04: Batch visibility, selection eligibility, labels, lock, snap, opacity, scale, compatible style copy/paste, move/group, union extent and export/remove with per-layer results.
- E05: Nonmodal styling panel, preview/apply/reset/cancel/undo, advanced symbology/filter/form/diagram tabs.
- E06: Stable single default label per feature, multipart policies, line placement, text/background/halo/format options, callouts, overrides.
- E07: Dynamic partial polygon labels with interior/hole correctness, stable identity, no repeated tile labels, bounded computation and camera settling.
- E08: Interactive label move/rotate/pin/hide/edit/reset, independent override storage and undo, no source geometry mutation.
- E09: Deterministic collision/priority across layers, obstacles, placement alternatives, label fit and explicit overlap.
- E10: Rule-based and zoom-dependent label content/style with validation and preview.
- E11: Independent render order, nested/empty groups, favorites, persistent expansion and filters, keyboard and drag operations.
- E12: Themes and schema-compatible style library, portable recipes, personal/shared scope and conflict handling.
- E13: Tile/GeoJSON parity for supported state; authorized tile attributes/filtering/label delivery, consistent selection identity, no accidental data exposure or double rendering.
- E14: QML/SLD documented interoperability subset, warnings and round-trip fixtures; no silent lossy import.
- E15: Regression tests, browser acceptance, security review and performance evidence on exact candidate.

## Acceptance
Layer states independent, mixed selection explicit, scope changes clear transient state, style copies never copy authorization/locks. No geometry changes from label editing. Invalid expressions and unknown fields rejected. All batch changes report unsupported or skipped layers. No cross-tenant preferences or requests. Dev deployment reversible. Production excluded.
Performance targets: 1000 layer tree rows, 100-layer batch, p95 local action <50 ms, 60 FPS physical GPU, no unexpected duplicated polygon label, no loss on reload. Current backend map-write limit 32 layers must be addressed with a separately tested contract change before claiming 1000-layer shared persistence.
