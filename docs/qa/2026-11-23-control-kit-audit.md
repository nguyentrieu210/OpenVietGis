# OpenVietGIS — Control Kit / parallel workstreams

**Date:** 2026-11-23 · **Base:** `main@5baade99dcb17024610993ee0c13e84af3d9e7b8`

## Objective

Stop uncontrolled feature growth, establish one authoritative release path, and provide
five independently reviewable workstreams without deleting, rebasing, or force-pushing
existing work. This document is an initial audit and task dispatch, **not an acceptance
certificate** and **not evidence that autonomous AI agents are installed**.

## Baseline evidence observed on VPS

- Source tree: 1,421 tracked paths, 460 commits; `main` clean, cached
  `origin/main` pinned to the same commit at audit time.
- Six cached remote branches have commits not contained in `main`:
  `003-spatial-catalog-crs`, `docs/r005-r015-grill`,
  `spec/016-app-shell-identity-ux`, `spec/017-gis-workspace-ux`,
  `spec/019-house-numbering-plugin`, `takeover/002-p2-secret-resume`.
  These may be historical or superseded; **do not automatically merge or delete**.
- Two frontend trees exist: `apps/web` (150 tracked paths) and
  `apps/web-next` (171 tracked paths). Current release manifest selects
  `apps/web-next`. Do not remove `apps/web` before coverage/dependency review.
- The backend contains 47 numbered migration files; house numbering contains
  105 tracked paths. Production schema and end-to-end functionality are **not verified**.
- Public `/` and `/app` deliver the index built for release
  `v2026.11.23-5baade9`; Cloudflare may append its own analytics beacon.
  There are 33 verified release web assets; `openvietgis-hn019-api` is active
  from the release directory, and its `main.py` bytes match the source tree.
- VPS has ~3.8 GiB total RAM and was using ~2.1 GiB swap during audit.
  Parallel heavy builds should be limited to one at a time.

## Five parallel workstreams

### Track A — Git source control / integration (P0)

**Scope:** branch inventory, unmerged commits, source-of-truth, release tags.
**Owned paths:** git metadata, `docs/qa/` reports (no feature code).
**Tasks:**
- [ ] Compare the six unmerged remote branches with `main`, identify unique changes
      and whether they are superseded; keep all old refs until evidence review.
- [ ] Record branch statuses: integrated / superseded / needs review.
- [ ] Define protected `main` policy (no force-push; PR + CI; tag releases).
**Done when:** every active branch has a disposition and reviewed evidence.

### Track B — Frontend canonicalization (P0)

**Scope:** `apps/web`, `apps/web-next`, web and web-next build pipeline.
**Tasks:**
- [ ] Produce route/feature parity matrix (editor, CAD, 3D, AI, layout, login,
      imports, plugins, offline/PWA, authorization).
- [ ] Confirm which frontend provides each public route; record service dependencies.
- [ ] Move only fully verified features to `apps/web-next`, then retire redundant
      frontend runtime in a **separately tested** deployment.
**Done when:** one frontend is documented as canonical, all critical flows pass,
and the legacy tree can be retired without feature regression.

### Track C — Backend, data and plugins (P0)

**Scope:** `apps/api`, `modules/house_numbering`, database migrations,
API contracts, auth/tenant isolation.
**Tasks:**
- [ ] Verify migration graph and runtime DB revision **read-only**; inventory
      migration/rollback implications before any write.
- [ ] Verify Core -> plugin dependency isolation and plugin activation per project.
- [ ] Exercise authorization isolation and plugin access for normal user accounts.
**Done when:** migration revision and plugin contract evidence are recorded;
no production data changed by test runs.

### Track D — Deployment / rollback (P0)

**Scope:** version manifests, Nginx, release folders, systemd and runtime artifacts.
**Tasks:**
- [x] Confirm source SHA, manifest, published HTML/assets and release API binary path.
- [ ] Remove divergent dev/preview paths from user-facing production **only after**
      routing/consumer analysis; never stop a service just because it looks old.
- [ ] Document deploy and rollback rehearsal using immutable release IDs.
**Done when:** frontend, backend, migrations and plugins pin to one known tested
release, with verified rollback.

### Track E — QA / acceptance evidence (P0)

**Scope:** CI, unit/contract/integration/e2e, live smoke, release gates.
**Tasks:**
- [x] Add a read-only structural release checker and offline unit tests.
- [ ] Run TypeScript/Python quality suites sequentially to respect memory limits.
- [ ] Produce feature-level working/partial/missing matrix with verifiable evidence.
- [ ] Establish release certification criterion; do not mark production-ready
      from structural checks alone.
**Done when:** CI is green, critical web workflows pass, and release evidence has
a commit SHA, date, environment and test output.

## Immediate rules

1. **Freeze new features** until P0 alignment/QA work is reviewed.
2. Create one branch/worktree **per real implementation agent** only when that
   agent is ready to run; never edit a shared dirty worktree.
3. Do not delete any old branch, database rows, Docker volume, service or bundle
   during this initial control phase.
4. Do not deploy from a dirty tree or a detached set of mismatched component builds.
5. Treat `tools/control_kit_audit.py` as an integrity check, **not** as functional,
   migration or security acceptance.

## Running the structural checker on the actual VPS

```bash
python3 tools/control_kit_audit.py \
  --repo /opt/openvietgis \
  --release-root /opt/openvietgis-releases \
  --public-root /var/www/openvietgis-hn019 \
  --service openvietgis-hn019-api \
  --public-url https://openvietgis.cloud
```

The tool is **read-only** and exits nonzero on structural FAIL. Warnings and
NOT_VERIFIED checks are never silently converted into acceptance. It uses cached
`origin/main`; do a vetted `git fetch` separately before relying on remote currency.

## Scope of this initial dispatch

Five **parallel audit executions** completed; this is not a claim that five
independent autonomous code-writing agents were launched. No production service,
data, or release symlink was changed.
