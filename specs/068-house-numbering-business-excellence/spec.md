# House Numbering Business Excellence

Status: planned / implementation in progress. Baseline main 5bfc8382; audit 2026-11-27.

## Scope
Extend existing Spec 019 and unfinished 051; preserve Bình Hưng evidence and local workflow. Legal issuance is blocked until the competent authority validates applicable rules and data. No invented approvals or destructive imports.

## Requirements and acceptance

### E01 — Domain identities
Separate parcel, building, entrance, address, household, evidence, certificate and plate; explicit cardinality and lineage.

### E02 — Versioned rules
Verify current legal texts and local policy with authority; version direction, parity, prefix, reserve and exceptions; enforce even step for parity sequences.

### E03 — Road graph
Canonical road IDs across hamlets; alley hierarchy, direction, topology and manual confirmation.

### E04 — Existing address registry
Separate certificate, plate, map label, survey, historic, proposed and approved values; source provenance and conflict resolution.

### E05 — Reconciliation
Implement Spec 051 six statuses/seven BRD groups against independently verified Bình Hưng reference fixture; no auto-approval.

### E06 — Field survey
Assigned mobile tasks, photos, offline outbox, retries, conflict review and scoped private data.

### E07 — Entrance and route assignment
Rank entrance access, existing address, neighboring anchors and geometry; ambiguous cases require review.

### E08 — Number allocation
Deterministic chainage, anchor gaps, suffix, existing reservations, reversed direction and nested alley rules; preview only until explicit commit.

### E09 — Exceptions
Corner buildings, multi-entrance, splits/merges, duplicate existing numbers, unnamed roads, unresolved address evidence.

### E10 — Quality and signoff
Independent dimensions of progress; 100% reconciled scope then signoff, audited reopen; prevent stale approval.

### E11 — Approval
Unknown eligibility cannot become approved; verified route/rule/evidence, duplicate protection and independent reviewer; transactional recheck on approval.

### E12 — Issuance and plates
Separate proposal approval from legal decision, certificate issuance, printing, delivery and installation; immutable audit and templates.

### E13 — QR and lookup
Stable revocable address ID, public data minimization, authenticated private records and deep-link tests.

### E14 — Address changes
Versioned effective dates, aliases, corrections, retirement and reissue; preserve historic searches.

### E15 — Dashboard
Define denominators and dimensions for coverage, reconciliation, signoff, issue, plate and rework.

### E16 — Map-first workflow
Select feature, inspect evidence, suggest, edit, compare, submit and review without switching GIS shell; complete 051 UX.

### E17 — Bình Hưng recovery
Locate previous dataset and backups read-only; verify provenance and restore only with approved plan; compare 30,805/1,858 historical counts without assuming current counts.

### E18 — Acceptance
Run domain, PostGIS, concurrency, offline and browser tests; witnessed pilot, 200-point reference comparison and 30k-scale fixture; authority signoff before issuance.

## Release gate
All requirements require executable evidence or explicitly documented external acceptance. No completion based on checkboxes alone.
