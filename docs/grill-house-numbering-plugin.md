# Grill — House Numbering Plugin (R014 child / proposed Spec 019)

Date: 2026-11-21 | State: discovery complete for available baseline; decisions proposed for review.
Target: OpenVietGIS official module `house_numbering`; not a Core feature.

## Evidence and baseline

- VPS source currently running: `/opt/openvietgis`, `integration/r006-r015-complete`, commit `c6d449f`.
- Isolated planning worktree: `/opt/openvietgis-house-numbering-019`, branch `spec/019-house-numbering-plugin`, created from verified main `c5cc5a9`. Do not merge or implement against missing prerequisites until R004–R014 contracts are promoted/verified. Active frontend worktrees are untouched.
- Legacy reference: private `nguyentrieu210/binh-hung-webgis` branch `008-khong-gian-lam-viec-gis`. Available via GitHub connector. VPS Git access to legacy private repo is not authenticated; do not extract or replicate credentials.
- Source documents retrieved (Git blob SHA, not commit):
  - `specs/001-so-nha-webgis/spec.md`: b928c8ba979be8d06660c5583aa63bb97cd11f0e (FR-001–067, US1–8).
  - `specs/001-so-nha-webgis/tasks.md`: 91167fc6a29616c216e011902bce8a71fc83f02c.
  - `specs/005-hoan-thien-nghiem-thu/spec.md`: 09a3efa951656c7c06d34901c7c4e3d28637b08d (FR-501–506).
  - `specs/006-kiem-tra-ra-soat-so-nha/spec.md`: 790155f49f7de78970e5740a1260fb5c1784203e.
  - `docs/brd-ra-soat-so-nha.md`: 99619cd22832bc7af1a0aa67775ff17a93cf38af (BR-01–08, 7 reconciliation categories).
  - `specs/008-khong-gian-lam-viec-gis/spec.md`: a3b7a1c45eaf9756626a8a0f8ddf6f544c6c005c.
  - First Grill reference: `docs/grill-decisions.md` at `001-so-nha-webgis`: cfa6e14f43609ae6574fef600924f60f5f6064c8.
- Local unpushed legacy Spec 009 is not available from VPS. Reconcile with it when a normal authorized source becomes available.
- Reference means transfer workflows, test cases and domain rules, not copy the old application's architecture or production data. No personal records imported for discovery/planning.

## Current R014 gap

Live branch has manifest v1.0.0, permissions, generic activation/record APIs and a deterministic odd/even string generator (`modules/house_numbering/workflow.py`). Integration test registers two modules and stores sample address plates, not a full house-number workflow. PlatformHub lists manifests only. No address geometry, route hierarchy, map click inspection, actual assignment workflow, approval, survey, reconciliation, or workbench UI demonstrated. Manifest presence is not installed-project proof or production acceptance.

## Decision register

| ID | Decision / default for new spec | Status |
| --- | --- | --- |
| D01 | Official plugin activated per Project; multiple organizations/projects, configurable areas; no hard-coded Bình Hưng. | Accepted architecture (Constitution/ADR-008); detail proposed |
| D02 | Use Core for spatial layers, map, CRS, editing, import/export, jobs, identity, history, field/offline; plugin owns domain schema, commands, views and policy. Core never imports a specific vertical module. | Accepted architecture |
| D03 | Workbench integrated into existing map and mobile workspace; area tree, map, synchronized list/inspector, tools; no separate legacy site. Module-specific UI only appears when enabled and authorized. | Proposed |
| D04 | House/building, parcel reference, current/historic area, road/alley hierarchy and stable IDs; geometry and CRS remain explicit, link to Core spatial resources. | Proposed |
| D05 | Existing/certified, drawn, surveyed, proposed and approved numbers distinct; no destructive overwrite of authoritative history. Manual confirmation required; versioned rules and source evidence. | Legacy-backed |
| D06 | Spatial order/side relative to oriented roads, allow approved direction policy, gap/subnumber rules, nested alleys, reserved numbers, preview/atomic apply; preserve approved and submitted numbers. | Legacy-backed |
| D07 | Import source mapping, precedence, encoding, provenance, rejected/matching rows and conflict queue; no silently guessed CRS. Legacy data only with authority and explicit import approval. | Legacy-backed |
| D08 | Personal information isolated; least privilege/project allocation, audited reads/exports, explicit lawful basis and consent where required, no personal data in standard maps/tiles/exports. Old broad read-access rules must not bypass tenancy or current authorization. No collection of actual CCCD until governance is confirmed. | Safety gate |
| D09 | Certificates and inspection reports use approved templates; draft watermark until authoritative rule and template verified, no auto-issuance. | Legacy-backed |
| D10 | No public citizen portal, city-system connector, arbitrary CAD engine reimplementation, or actual data migration by default; register as scope change if needed. | Proposed |
| D11 | Keep R014 reference module working without forcing plugin on existing projects; add compatible version/migration with rollback and independent disable/re-enable. | Proposed |

## Capability mapping (legacy to plugin)

| ID | Workstream and user outcome | Legacy evidence | Core contract dependencies |
| --- | --- | --- | --- |
| HN01 | Activation, scoped permissions, localized menu/workbench and persistent project config | Spec 008 US1; current R014 | R002/R004/R014 |
| HN02 | Area tree: administrative area, historic boundaries, streets, alleys/nested and number-only alleys; map/list scope, counts, assignment | 001 FR-007/044/060–066; 008 US1 | R003/R004/R006/R007 |
| HN03 | Spatial house/building registry: pin, move, remove, select, parcel link, current/old/new/approved numbers, special values/raw evidence | 001 FR-014/021/061–063 | R003/R004/R006 |
| HN04 | CAD/Excel/GIS import: mapped layers, Unicode/TCVN3/VNI, source precedence, preview, checks, reconcile counts, unmatched/manual queue; historic/current layers | 001 FR-001–007/046/058–059; 006 FR-608–611/617–618 | R003/R005/R008/R012 |
| HN05 | One-click inspect any point: parcel/area/house, certificates/candidate matches, nearest oriented street, neighbors same side, grid and nearby evidence; explicit no match/ambiguous result | 006 FR-601–604; BR-01–08 | R003/R004/R007/R008 |
| HN06 | Numbering: odd/even, direction, gap filling, suffix A/B or dash-n, nested alley prefix, bulk pin/assign, grid, preview then apply, insert/shift, immutability of approved/submitted numbers, reserved slots | 001 FR-015–020/042–045/060–066; 005 FR-505; 006 FR-603/613–614 | R004/R006/R007/R008/R014 |
| HN07 | Reconciliation of certificate/CAD/latest survey: exclusive result groups, evidence, manual resolution, matching old before new where unambiguous, estimated location labeled | 006 FR-608/612/617–618; BRD §7 | R003/R006/R007 |
| HN08 | Proposal state machine draft → submitted → approved/rejected, reasons, reviewer role, duplicate blocking, history, plaque installation and non-eligible reasons | 001 FR-014–021/043; 005 US4/5 | R002/R006/R011/R014 |
| HN09 | Field survey web/mobile: GPS, photo, conditions, optional household/consent, offline queue, idempotency, conflicts, delegated draft recovery | 001 FR-009–013/033–039/055–056; 005 US6 | R002/R006/R011/R013 |
| HN10 | Progress, targets, assignments, survey diary by area/date, Excel and quality counts | 001 FR-048–050 | R007/R011/R013 |
| HN11 | Inspection sheet Word (cadastral map, satellite if licensed/available, numbering grid), dossier catalog and audit; certificates PDF and plates, batch issue/void/reissue only as permitted | 001 FR-051–054; 006 FR-605–610 | R009/R010/R014 |
| HN12 | Search current/old/proposed/address/road/area/coordinates; source-linked records; exports by area/layers/fields, redacted package vs authorized original handover, metadata and geometry round trip | 001 FR-022–028/040; 005 FR-501–504; 008 US2–4 | R003/R005/R007/R009/R010/R012 |
| HN13 | Restricted household/CCCD, purpose-limited access, no standard GIS leakage, audit access and retention/case authorization, no live PII in tests | 001 FR-029–039/046/055–057; 006 FR-607/616 | R002/R011/R014 |

All listed workstreams are target scope, not implementation claims. Each Core dependency must be verified against runtime capability; any gap is a linked prerequisite, not a duplicate implementation in plugin.

## Rules to carry into executable acceptance tests

- A point with an existing number returns it without mutation; between 100 and 106 propose 102 if unused; between 100 and 102 propose 100A, then 100B if A used. Alternative convention 100-1 configurable. Same side only; nested alley prefix e.g. 155/22, no nearest-route guess past configured distance. Direction/parity are explicit and configurable; defaults require local policy confirmation.
- Between reserved/submitted/approved numbers, no silent shift; when no slot exists return an actionable conflict. Preview does not write records; apply compares revision and commits all or none.
- Seven reconciliation outcomes: matched, number mismatch, CAD vs certificate mismatch, new building, no longer exists, not surveyed, certificate unmatched. Unmatched certificate is counted separately from mutually exclusive building categories; totals reconcile.
- Old/new source labels preserved; nonnumeric special numbers retained; ambiguous source matches require manual confirmation.
- Import cannot silently accept unknown CRS, PII layers, duplicate replay, or uncertain identification; authorized raw data remains separately protected.
- Cancel/reload does not change data. Multiuser stale edits fail without lost updates; unauthorized users cannot approve or access nonassigned/project records even by direct endpoint. Module disable must not delete records.
- Approved-only certificates, repeat print with audit, changed approved number flags prior certificate for reissue; missing satellite image clearly stated rather than fabricated.
- Offline replay test: 50 draft submissions plus photos after reconnect, zero lost or duplicate. Out-of-scope expired drafts require authorized delegated recovery.
- Standard output includes zero names, phone or CCCD; authorized sensitive access is separately logged.
- Complete browser flow: enable on two isolated projects, create route and houses on real map, inspect, suggest and preview, apply, review and approve, restart/reload, export and re-open. Test inactive project, failed role, competing edit, missing CRS, duplicate, deactivation, rollback. Verify clean Core with module absent and run regression against Land Asset module.

## Order of delivery

1. HN01–03: official module installed per project, map/context, real spatial-backed data and role gates.
2. HN05–06 + HN08: daily inspect/number/approve workflow (full vertical slice), backend and UI.
3. HN04 + HN07: import and reconcile three sources with test data.
4. HN09–10: field/offline and progress once contracts pass.
5. HN11–13: outputs, search, handover, protected-data implementation (security boundary starts phase 1, collection remains gated).
6. Converge each slice with evidence: acceptance scenarios, integration against actual test DB, browser UI, permissions, CRS, geometry, revision, restart, export verification; no R014 completion implied by counting specs.

## Remaining decisions / owner gates

- Confirm actual rulebook for issuing numbers, jurisdiction-specific CRS/control points and local numbering policy before official issuance; legacy BRD references are not current legal verification.
- Confirm project authorization to import Bình Hưng original files/PII and actual templates; no live data copied as part of this kickoff.
- Validate legacy Spec 009 and additional branch/source changes when they are available.
- Confirm UI contract dependencies are reviewed/promoted; existing frontend integration/audit worktrees remain untouched.

## Spec Kit execution status

- Constitution 1.0.0, ADR-008, product and roadmap read; no constitutional change required.
- Intended child `specs/019-house-numbering-plugin/spec.md` under R014; 016–018 reserved by existing work.
- `/speckit-specify` prerequisite attempted using required template resolver:
  `pwsh -File .specify/scripts/powershell/resolve-template.ps1 spec-template -Json`
  Initial result: `pwsh: command not found`. Resolved with isolated checked PowerShell 7.6.10 under /opt/toolchains; template resolver, setup-plan, setup-tasks and check-prerequisites now pass. See child spec and plan at specs/019-house-numbering-plugin/.
- Specify → clarify → plan → checklist → tasks → analyze completed for child 019; pure domain implementation and tests passed. Integrated implementation remains blocked by Core dependency gate; no production deployment or live code change performed.
