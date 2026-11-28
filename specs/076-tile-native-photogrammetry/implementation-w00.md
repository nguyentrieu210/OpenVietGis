# SPEC 076 Revision 2 — Wave 00 implementation checkpoint

Status: PARTIAL; all real photogrammetry capabilities remain disabled. The full 16-document Revision 2 kit is not yet imported in this branch; this is an implementation checkpoint, not a replacement.

## Pinned platform inputs
- Desktop 074 base: 1fe44b3457090ceef2320fa1f364bbaf43ea7307.
- Mobile 073 observed branch tip: 245651a016bb0fa059205516d5dedad8ed43f003. Desktop itself pins b2c7a1fcbf80ae0da70a4c8e495efd25f2cb3517 for shared engine/storage/spatial dependencies; do not silently repin.
- Plugin 075 observed tip: 62368bea3050037c48b9fe692ffce3cbec4f5fea. Not merged or assumed accepted.
- main observed: a4ae85a5e3d956de1870dcf65ff3fb21da7f0367. PR must initially target 074, not main, to avoid importing unrelated changes.

## ADR proposal and ownership
Reuse React/Tauri and existing data plane, project store and local runtime. Place provider protocol in ovg-processing (existing serde dependencies; no new lockfile or independent authoritative storage). Desktop bridge must expose typed commands, opaque handles, project asset references and bounded data-plane reads. No arbitrary shell/SQL or raw file paths from UI. Worker execution and packaging remain unimplemented. Host verifies signatures/licenses and artifact digest, owns grant and lease registry, performs actual resource admission and OS containment. Protocol validation alone does not enforce process limits or sandboxing. No network by default; remote worker requires explicit opt-in.

## W00 status
| Task | State | Evidence or remaining gate |
|---|---|---|
| T01 | partial | pinned inputs and proposed ADR; reconcile dependencies and review adapter |
| T02 | implemented, unverified | typed negotiation and unit tests; run CI and integration, define full operation contracts |
| T03 | blocked | COLMAP unavailable on deployment-only VPS; real sparse/dense fixture and peak metrics required |
| T04 | open | inventory exact packages, models, SPDX, hashes and license approvals |
| T05 | open | real offline Windows IPC worker and preview required |
| T06 | partial | benchmark contract below; rights-cleared dataset and real baseline needed |
| T07 | partial | opaque handle and lease checks; host grant enforcement and malicious inputs still required |
| T08 | partial | release owner lanes and disabled-by-default policy below; no production capability advertised |
| T09 | open | verify legacy projects and rollback byte equality; no migrations introduced |

## Qualification contract
Measure same fixture hashes, machine, driver, OS, GSD, controls, quality and output extent on baseline and candidate. Report wall time, RSS and VRAM peaks, scratch/output bytes, XY/Z independent checkpoint error, nodata, seam residual, reconstruction completeness; record failures and cold/warm runs separately. No synthetic benchmark presented as real. COLMAP sparse/dense and native GPU providers must be independently qualified per operation, hardware and license. Windows reference is mandatory for desktop vertical slice. Do not mark W00 complete or enable processing flags until real evidence passes.

## Release lanes (proposed)
Core/quality W00–W04: platform and processing owners; W05–W07: geometry/survey; W08 and W11: ortho; W09–W12: dense/surface; W13: desktop; W14: revision; W15: AI/multispectral; W16: distributed; W17: independent acceptance. All capabilities disabled until their release lane passes; no cloud deployment in this scope.
