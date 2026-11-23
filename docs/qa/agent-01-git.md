# Agent 01 — Git Governance & Branch Inventory (P0/A)

**Baseline audited:** `main@fc8b8de` (post-merge of PR #2, #3, #14, #15, #16, #18, #19; pinned historical baseline `5baade99dcb17024610993ee0c13e84af3d9e7b8`).  
**Date:** 2026-11-23 · **Ref mutation:** Read-only audit (no force pushes or unreviewed deletions).

## 1. Open PR Queue Status
All 6 previously open PRs (#2, #3, #14, #15, #16, #19) were verified (Ruff = 0, MyPy = 0, `check_scope.py` = 0, unit/contract/architecture tests PASS) and merged into `main`, with their head branches deleted. **0 open PRs remain.**

## 2. Remote Branch Disposition Matrix (`git rev-list --left-right --count origin/main...<branch>`)

| Remote Branch | Behind `main` | Ahead of `main` | Category | Recommended Disposition |
|---|---:|---:|---|---|
| `origin/feature/028-windows-v8i-receiver` | 46 | 8 | Active feature (Windows V8i receiver) | **Review / open PR** when Windows COM receiver testing is ready |
| `origin/docs/r005-r015-grill` | 508 | 81 | Historical spec/grill discussion docs | **Archive / tag & prune** after confirming no unported decisions |
| `origin/003-spatial-catalog-crs` | 510 | 10 | Superseded early R003 branch | **Archive / prune** (R003 landed via `integration/003-on-verified-main`) |
| `origin/spec/016-app-shell-identity-ux` | 332 | 1 | Spec draft (1 commit ahead) | **Review & archive** |
| `origin/spec/017-gis-workspace-ux` | 332 | 1 | Spec draft (1 commit ahead) | **Review & archive** |
| `origin/spec/019-house-numbering-plugin` | 332 | 2 | Spec draft (2 commits ahead) | **Review & archive** |
| `origin/takeover/002-p2-secret-resume` | 439 | 1 | Historical R002 takeover snapshot | **Archive / prune** |
| `origin/feature/019-house-numbering-runtime` | 297 | 0 | Merged (`0` unique commits) | **Safe to delete** |
| `origin/feature/026-integration-hub-policy` | 52 | 0 | Merged via PR #18 (`0` unique commits) | **Safe to delete** |
| `origin/fix/018-confirmed-blockers` | 202 | 0 | Merged (`0` unique commits) | **Safe to delete** |
| `origin/integration/*` (4 branches) | 305–332 | 0 | Merged (`0` unique commits) | **Safe to delete** |
| `origin/release/beta-20261123` | 166 | 0 | Release tag candidate (`0` unique commits) | **Convert to lightweight/annotated tag & delete branch** |
| `origin/takeover/002-*` (8 branches, 0 ahead) | 424–443 | 0 | Merged historical R002 (`0` unique commits) | **Safe to delete** |
| `origin/validation/r003-*` (6 branches, 0 ahead) | 332–359 | 0 | Merged historical R003 (`0` unique commits) | **Safe to delete** |

## 3. Branch Protection & CI Gate Gaps
- `.github/workflows/ci.yml` defines 3 jobs (`backend`, `validate`, `secrets`). Both `ruff` (PR #16) and `check_scope.py` (commit `a58db74`) are now fixed to 0 violations on `main`.
- **Gap (maintainer action on GitHub Settings):** Enable GitHub Branch Protection on `main` requiring `backend`, `validate`, and `secrets` status checks + disallowing force-pushes.
