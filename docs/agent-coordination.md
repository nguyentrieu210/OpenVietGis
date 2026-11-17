# OpenVietGIS agent coordination ledger

Updated: 2026-11-18 (Vietnam time)
Owner: coordinator. This is an operational record, not feature acceptance.
Scope approved: R005–R015 first-slice matrix in [decision register](r005-r015-grill.md), commit f86c64f.

## Execution and resumption rules

- Each feature draft has its own branch and worktree from verified main ea6db79. New features must use current verified main; existing branches sync only at a clean checkpoint under the integration owner.
- The product owner authorized the tracked Spec Kit PowerShell resolver/scaffold scripts with **process-only RemoteSigned**. The coordinator tested this on R006. Never use Bypass or modify machine/user policy; stop on a separate permission denial.
- Continue approved work and review without repeat approval. Ask only for material unresolved product decisions, new scope, protected permission failures, or risk requiring user choice.
- The product owner has authorized full roadmap execution with parallel agents and autonomous continuation. Current checkpoint remains independent cross-spec review and prerequisite validation; only move a feature to Plan/Tasks/Implement after its dependency and research gates are accepted. Draft acceptance is not implementation convergence. Do not modify ROADMAP.md concurrently; integration owner controls shared files and merge gates.
- Spec Kit active-feature state must remain isolated per worktree. Do not overwrite another agent's active changes. Preserve existing work; do not recreate worktrees.
- Agent orchestration is resumable from this ledger. The user enabled an hourly OpenVietGIS Agent Coordinator Scheduled Task on 2026-11-18; actual Local MCP/tool availability must be checked at each run. The schedule does not guarantee continuous execution. Avoid overlap with other active workers.

## Draft coordination snapshot

| R | Branch | Worktree | Draft status |
|---|---|---|---|
| 001 | main | C:/code/openvietgis-main | Foundation converged and integrated |
| 002 | 002-identity-organization-tenancy | C:/code/openvietgis | Active dirty worktree, 43/115 tasks ticked at 01:56; owner independent, no takeover |
| 003 | 003-spatial-catalog-crs | C:/code/openvietgis-r003 | Draft clarified: 3b8781c; R002 and evidence gates open |
| 004 | 004-map-workspace | C:/code/openvietgis-r004 | Draft clarified: 4c32816; R003 dependency gate open |
| 005 | 005-import-export | C:/code/openvietgis-r005 | Draft audited and revised: ea96cfa |
| 006 | 006-editing-change-history | C:/code/openvietgis-r006 | Draft audited and revised: b0b5f7b |
| 007 | 007-attribute-spatial-query | C:/code/openvietgis-r007 | Draft audited and revised: 51d61ab |
| 008 | 008-processing-jobs | C:/code/openvietgis-r008 | Draft audited and revised: d4eae02 |
| 009 | 009-cartography-output | C:/code/openvietgis-r009 | Draft audited and revised: 5ba4699 |
| 010 | 010-external-sources | C:/code/openvietgis-r010 | Draft audited and revised: 854fcd0 |
| 011 | 011-team-collaboration | C:/code/openvietgis-r011 | Draft committed: 533af9a |
| 012 | 012-cad-interoperability-precision | C:/code/openvietgis-r012 | Draft committed: 751ef91 |
| 013 | 013-field-offline | C:/code/openvietgis-r013 | Draft committed: cfdeed7 |
| 014 | 014-module-framework | C:/code/openvietgis-r014 | Draft audited and revised: 7bb9879 |
| 015 | 015-production-readiness | C:/code/openvietgis-r015 | Draft audited and revised: 3831db0 |

## Next integration checkpoint

Verified 2026-11-18: all 11 R005–R015 draft commits have parent ea6db79, clean worktrees, only files under their respective specs directories, and clean Git whitespace checks. All draft authors reported successful tracked template resolution under authorized process-only RemoteSigned; R007 was reverified by coordinator. No source code or shared ROADMAP changed, no feature branches merged. Independent cross-spec acceptance/research gates remain open. See [cross-spec contract gates](cross-spec-contract-gates.md), [R003 CRS evidence research](r003-crs-evidence-research.md), and [R002 read-only live audit handoff](r002-live-audit-handoff.md); active R002 owner must reconcile these findings at a clean validation checkpoint. R002 actively implementing identity/tenant RLS and tests in its separate worktree; do not interfere. At R002 convergence and promotion to verified main, synchronize R003 from that baseline and complete its prerequisites/Plan/Tasks; thereafter R004 and R005 before dependent runtime slices. No downstream implementation without accepted prerequisite contracts and separate integration review.
