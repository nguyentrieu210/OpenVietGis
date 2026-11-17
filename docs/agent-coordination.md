# OpenVietGIS agent coordination ledger

Updated: 2026-11-18 (Vietnam time)
Owner: coordinator. This is an operational record, not feature acceptance.
Scope approved: R005–R015 first-slice matrix in [decision register](r005-r015-grill.md), commit f86c64f.

## Execution and resumption rules

- Each feature draft has its own branch and worktree from verified main ea6db79. New features must use current verified main; existing branches sync only at a clean checkpoint under the integration owner.
- The product owner authorized the tracked Spec Kit PowerShell resolver/scaffold scripts with **process-only RemoteSigned**. The coordinator tested this on R006. Never use Bypass or modify machine/user policy; stop on a separate permission denial.
- Continue approved work and review without repeat approval. Ask only for material unresolved product decisions, new scope, protected permission failures, or risk requiring user choice.
- The current run is **Specify/Draft only**, not Plan/Tasks/Implement. Draft acceptance is not implementation convergence. Do not modify ROADMAP.md concurrently; integration owner controls shared files and merge gates.
- Spec Kit active-feature state must remain isolated per worktree. Do not overwrite another agent's active changes. Preserve existing work; do not recreate worktrees.
- Agent orchestration is resumable from this ledger even if the chat does not automatically wake; no claim of an unattended process.

## Draft coordination snapshot

| R | Branch | Worktree | Phase at start of resumed run |
|---|---|---|---|
| 005 | 005-import-export | C:/code/openvietgis-r005 | Existing draft, reverify then commit |
| 006 | 006-editing-change-history | C:/code/openvietgis-r006 | Resolver confirmed RemoteSigned; resume draft |
| 007 | 007-attribute-spatial-query | C:/code/openvietgis-r007 | Draft commit a2ebe26; reverify resolver |
| 008 | 008-processing-jobs | C:/code/openvietgis-r008 | Queued, resume from clean worktree |
| 009 | 009-cartography-output | C:/code/openvietgis-r009 | Existing draft, reverify then commit |
| 010 | 010-external-sources | C:/code/openvietgis-r010 | Existing draft, reverify then commit |
| 011 | 011-team-collaboration | C:/code/openvietgis-r011 | Queued, resume from clean worktree |
| 012 | 012-cad-interoperability-precision | C:/code/openvietgis-r012 | Queued, resume from clean worktree |
| 013 | 013-field-offline | C:/code/openvietgis-r013 | Queued, resume from clean worktree |
| 014 | 014-module-framework | C:/code/openvietgis-r014 | Resume draft |
| 015 | 015-production-readiness | C:/code/openvietgis-r015 | Resume draft |

## Next integration checkpoint

Verify each exact commit and clean worktree, traceability and checklist. Record draft status separately from dependency acceptance; R002 through R005 critical path and downstream gates remain binding. No merge or implementation without separate integration review.
