# Phase 0 — Source-backed research
Baseline OpenVietGIS main `9c71db0e565df45af9b18dee2b44fdcc6e160fb8`; R031 PR #21 head `8bbc3e0e3919c59c23dcbc2ab17c5d63fd08a7f2` (draft, not merged); OpenVietCAD live `2bc3537346ff2a844039685186a1b32da7149d6f`.
Evidence: [R031 contract](https://github.com/nguyentrieu210/OpenVietGis/blob/8bbc3e0e3919c59c23dcbc2ab17c5d63fd08a7f2/specs/031-engine-first/contracts/engine-adapter.md); [CAD build](https://github.com/nguyentrieu210/OpenVietCAD/actions/runs/45347995900); prior production synthetic DWG/DXF relay roundtrip passed; this does not certify full functionality or sync.

| Decision | Rationale / alternative rejected |
|---|---|
| Extend R031, R027, R029 | Existing contract, binding and durable cursor; reject parallel bus/core |
| Separate native CAD bytes from canonical GIS | Preserve curves, blocks, solid and drafting detail; reject forcing every native entity into GeoJSON |
| Explicit coordinate and precision policy | Reject inferred EPSG or VN-2000 zone/vertical datum |
| Preview, then versioned commit | Reject blind whole-document or direct SQL edits |
| Staged file publication with receipt/recovery | Object storage and DB not automatically atomic; reject claiming distributed atomic commit |
| Manual binding first, then qualified deltas | Native CAD save may serialize full file; reject promising per-entity write before measured engine support |
| R031 dependency gate | Draft contract may evolve; no changes to its source branch |
| Separate CI/runtime | GIS changes do not require Rust; compare compression/optimization against fixture performance, avoid arbitrary profile change |
| File-level access boundary | Arbitrary CAD content cannot be safely redacted by hidden layers; deny or create reviewed sanitized derivative |
| Offline draft vs accepted | R029 managed mode online-only; no authoritative offline mutation until approved separate policy |

Open research spikes: native handle stability across DWG versions and split/merge; CAD incremental API capability; independent VN-2000 controls; 3D vertical datum/mesh support; real cold/warm transfer and memory; source/license compatibility. Resolve by tasks, not assumptions.
