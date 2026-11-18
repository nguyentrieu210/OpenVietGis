# Roadmap execution gates — R001–R015

2026-11-18 • Operational execution register, not a new product scope or acceptance record.
Authoritative requirements: verified main ROADMAP.md, Constitution, accepted ADRs, approved first-slice Grill register and each feature's Spec Kit artifacts.
Current verified main: ea6db79. R002 takeover local/remote: 276c524. GitHub public CI: 44215960147 (backend/secrets PASS; frontend dependency-age gate FAIL). This is not full R002 acceptance.

## Dispatch queue (dependency waves, not a calendar promise)

| Wave | Candidate | Required integrated capability | Minimum additional gate before promotion |
| --- | --- | --- | --- |
| 0 | R001 | None | Already converged and integrated; retain original acceptance invariants. |
| 1a | R002 P1 | R001 | Supported frozen dependency install, complete frontend/real browser identity/tenant/revocation evidence and T080 independent review; do not start P2 beforehand. |
| 1b | R002 P2 | T080 | Service credential ceilings/revocation, authorized audit read, atomic bootstrap, parity, full migration/backup, sample retirement, SC timings, full root validation and T115 convergence. |
| 2 | R003 | Integrated R002 | Stable source/layer/field IDs, explicit metadata/content/schema revisions, accepted lawful CRS profile/control evidence, current tenant authorization, bounded reference read; unsupported profile unavailable rather than guessed. |
| 3 | R004 | Integrated R003 | Map workspace, layer order/visibility, feature detail/select, revision-aware configuration save/reopen and lost-ack recovery; renderer-neutral command/capability. |
| 4 | R005 | Integrated R003/R004 | Approved format/CRS matrix; durable import/export jobs, checksums, atomic publish, cancellation/retry, independent GIS round-trip and cross-tenant denial; M1. |
| 5 (parallel eligible) | R006 | Integrated R005 | Atomic changeset, draft conflict and replay, typed geometry/attribute edits, before/after evidence, audit. |
| 5 (parallel eligible) | R007 | Integrated R004/R005 | Typed attribute and spatial filters, bounded deterministic paging, pinned revisions and map/table selection. |
| 5 (parallel eligible) | R009 | Integrated R004/R005 | Reproducible style/legend and printable scale/CRS output, controlled data disclosures. |
| 5 (parallel eligible) | R010 | Integrated R003/R004/R005 | One approved read-only adapter slice at a time; SSRF/origin/credential/CRS/licensing/cache/revocation gates. |
| 6 (parallel eligible) | R008 | Integrated R005/R007 | Shared job framework and approved algorithm slices; immutable pinned inputs, new outputs, no silent mutation. |
| 6 (parallel eligible) | R011 | Integrated R002/R006/R007 | Concurrent changes and scoped review/activity, no duplicate ACL or last-write-wins. |
| 6 (parallel eligible) | R012 | Integrated R005/R006/R007 | First DXF and precision slices; explicit units/entity/CRS/loss matrix; DWG only as separate licensed gate. |
| 6 (parallel eligible) | R014 | Integrated R006/R007 (+R008 if its module needs processing) | Versioned extension/permission/command lifecycle; two separate reference module workflows, portable module data, no unsafe code execution. |
| 7 (parallel eligible) | R013 | Integrated R005/R006/R011 | Authorized bounded offline package/sync, revision conflict/replay, expiry and protected local storage controls. |
| 7 (parallel eligible) | R015 | Integrated R002/R005/R006/R011 | Verified release operations profile, upgrade/restore, resource fairness, tenant safety, M3; commercial/billing is separate approved slice, not automatically required for operations. |

Parallel eligibility means dependencies of the individual row are met, not that the entire named wave must finish first. Shared contracts/migrations and final integration have one owner; each implementation worker uses separate branch/worktree from exact verified main and its isolated active Spec Kit state. Reassess all dependents after each contract change. No downstream implementation while its prerequisites remain unaccepted.

## Per-feature evidence and GitHub workflow

1. Recheck active owners and clean worktrees; reconcile old draft spec artifacts only, never wholesale-merge stale branches. Read authoritative scope and resolve material product/research decisions before Plan/Tasks.
2. Specify/Clarify/Plan/Tasks, trace requirements and acceptance scenarios, then implement/test in isolated worktree. Use actual dependency and evidence gates, not task ticks.
3. For authorized public CI, review committed file inventory/history and Gitleaks before publishing only the intended clean source branch. No private coordination ledger, local credentials, data dumps, user datasets, other worktrees or production access. GitHub Actions uses synthetic isolated fixtures and exact branch SHA. Do not weaken supply-chain or secrecy rules; public logs are sanitized.
4. Require independent review, actual mandatory CI checks for exact head, real E2E where required, measured evidence, policy compliance and Spec Kit convergence. Fix failures on the source branch, rerun; no pass inferred from a skipped job. If a learner/user exercise needs a human participant or material approval, report it rather than fabricate evidence.
5. Integration owner verifies main clean, dependency order, exact source SHA and no competing integration, performs local integration, reruns post-integration gates where required and only then publishes verified main. Never force-push or promote a red/unverified head.
6. Update the private ledger and public roadmap links with evidence; distinguish drafting, planned, implemented, verified, integrated and full accepted. Scope slices remain only those approved, with separate gates for expanded scope.

## Current resumption

R002 browser gate: TanStack @5.108.0 was rejected by ordinary pnpm minimumReleaseAge; retry only when eligible (approximately 20:44 ICT on 2026-11-18, check actual policy). Latest GitHub CI 44215960147, exact 276c524. Backend and secrets green, frontend red at frozen install. If exact head unchanged and no active duplicate run, rerun failed job after eligibility; if new legitimate code checkpoint, review/commit/push that exact clean branch and run CI. P1 T080 still open.

R003 independent CRS research may continue without implementation. R003–R015 draft worktrees are in the coordination ledger; do not claim integrated code until dependency-gated verification. Stop only when all approved R001–R015 slices are accepted, or a material blocker remains and no independent safe work can progress.
