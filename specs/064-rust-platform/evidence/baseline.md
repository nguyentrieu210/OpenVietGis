# Spec 064 Baseline, Git Refs, Live Artifact & Schema Reconciliation (T001)

- **Captured At**: `2026-11-25T16:45:39Z` (`2026-11-25 23:45:39 +0700`)
- **Host**: VPS1 `Milion` (`103.77.243.154`), `Linux 6.12.41+deb13-amd64`
- **Worktree**: `/opt/openvietgis-rust-platform-spec`
- **Branch**: `064-rust-platform`
- **Requirement**: `FR-001` (preserve Git history; explicitly reconcile all in-scope main/dev commits, live artifact commit, unmerged features, and dirty worktrees; no force push)
- **Live Services Touched**: None (100% read-only inspection of live processes, worktrees, and PostGIS containers; local lightweight Git tag created only).

---

## 1. Authoritative Git References & Live Artifacts

| Reference / Artifact | Path / Origin | Branch / Type | Exact Commit SHA | Short SHA | Relation to `main` (`3e78ac1`) | Disposition for Spec 064 |
| --- | --- | --- | --- | --- | --- | --- |
| Spec 064 working branch | `/opt/openvietgis-rust-platform-spec` | `064-rust-platform` | `babcdf9f00578ab336fb4404c90186fa55be9b51` | `babcdf9` | 2 commits ahead, 0 behind | Active Spec 064 specification & implementation branch |
| Local `main` (Spec 064 reference baseline) | `/opt/openvietgis-pre-rust-e2519e4` | `main` (clean) | `3e78ac142acfc050ec22d30da5c44ad0e6b8459a` | `3e78ac1` | Baseline (0 ahead, 0 behind); 13 ahead of `origin/main` | **Pinned behavioral oracle & rollback baseline** (`rollback/064-baseline-3e78ac1`) |
| Remote `origin/main` | `refs/remotes/origin/main` | remote tracking | `1aa8dcb2fa76bbedfe8a08a33da3d7bd5fa9fddc` | `1aa8dcb` | 0 ahead, 13 behind `main` | Last pushed GitHub `main` (`Merge pull request #54 from nguyentrieu210/feat/060-chat-rooms`, 2026-11-25 22:06:32 +0700) |
| Live release artifact (HN019 / unified preview API) | `/opt/openvietgis-releases/v2026.11.25-1aa8dcb-chat-rooms` | Non-git release snapshot | Matches `1aa8dcb2fa76bbedfe8a08a33da3d7bd5fa9fddc` | `1aa8dcb` | 13 behind local `main` | Active production/preview API on `127.0.0.1:8019` (`openvietgis-hn019-api.service`, 2 uvicorn workers) backed by `hn019-preview-postgres:54340/openvietgis_test` |
| Live `/opt/openvietgis` primary repo checkout | `/opt/openvietgis` | `docs/readme-community-legal-20261124` (clean) | `96897a2371f89ee3c75cecf57473fc387818f647` | `96897a2` | 0 ahead, 302 behind `main` | Serves legacy `openvietgis-api.service` (`127.0.0.1:8000`, `dev-postgres-test-1:54330/openvietgis_live` at `0019_extension_history`) & legacy Vite (`5175`); **DO NOT modify** |
| Isolated Spec 063 WKB worktree | `/opt/openvietgis-rust-spatial-core` | `feat/rust-spatial-core` (**dirty: 5 items**) | `3e78ac142acfc050ec22d30da5c44ad0e6b8459a` | `3e78ac1` | Same commit as `main` + uncommitted Spec 063 WKB files | **Explicitly EXCLUDED from Spec 064 baseline** (separate Spec 063 experiment: `apps/api/native_wkb/`, `specs/063-rust-native-wkb/`, `spatial_repository.py`, `test_feature_persistence.py`, `ROADMAP.md`) |
| Spec 064 commits (`main..064-rust-platform`) | `/opt/openvietgis-rust-platform-spec` | `064-rust-platform` | `a23771eb709d661a7d4ec8539298977cbdfb4149`, `babcdf9f00578ab336fb4404c90186fa55be9b51` | `a23771e`, `babcdf9` | 2 spec commits ahead of `main` | Spec 064 documentation & static inventory baseline |

### 1.1 Exact 13 Commits Between `origin/main` (`1aa8dcb`) and Local `main` (`3e78ac1`)

Command: `git log --format="%H %h %ai %an | %s" origin/main..main`

| # | Short SHA | Full SHA | Timestamp | Author | Subject | 064 Scope Disposition |
| --- | --- | --- | --- | --- | --- | --- |
| 1 | `3e78ac1` | `3e78ac142acfc050ec22d30da5c44ad0e6b8459a` | 2026-11-25 23:29:21 +0700 | OpenVietGIS VPS Builder | `refactor(frontend): consolidate client, route compatibility and canonical unit tests` | Included in 064 baseline (`3e78ac1`) |
| 2 | `157e934` | `157e934f0aa5916e559a7c4be66b873ed113bb2d` | 2026-11-25 23:21:59 +0700 | OpenVietGIS VPS Builder | `refactor(frontend): internalize workbenches and modernize processing and offline shell` | Included in 064 baseline |
| 3 | `e2519e4` | `e2519e481af128fb2381f4ec0e6808ed998a0096` | 2026-11-25 23:15:04 +0700 | OpenVietGIS VPS Builder | `spec: single canonical frontend and professional desktop UX` | Included in 064 baseline (Spec 062) |
| 4 | `7071d19` | `7071d19050e51dcba31c7a97ac8941188eabfe30` | 2026-11-25 23:13:16 +0700 | OpenVietGIS VPS Builder | `refactor(ui): unify desktop dialog keyboard behavior and legacy workbench layout` | Included in 064 baseline |
| 5 | `e40024f` | `e40024f982db35d797ac55f9cf2da1cbf5b02a7b` | 2026-11-25 23:01:13 +0700 | OpenVietGIS VPS Builder | `fix(gis): match QGIS Space-only cursor panning` | Included in 064 baseline |
| 6 | `4e67dea` | `4e67dea0de94ff58b44fcf37432d178a5e907e08` | 2026-11-25 22:57:03 +0700 | OpenVietGIS VPS Builder | `fix(gis): extend temporary pan to map overlays and capture pointer` | Included in 064 baseline |
| 7 | `891bebf` | `891bebff8fa283015d5a8bd5b674d699975c4b4c` | 2026-11-25 22:39:43 +0700 | OpenVietGIS VPS Builder | `fix(gis): prioritize Space panning when cursor is over map` | Included in 064 baseline |
| 8 | `469918f` | `469918f55946b747a556d22986e3f842d292cb18` | 2026-11-25 22:34:42 +0700 | OpenVietGIS VPS Builder | `fix(cad): single row toolbar and isolated compressed dev engine` | Included in 064 baseline |
| 9 | `cecf505` | `cecf5054ca5f1a54e42ff33719b09e4800bfdbea` | 2026-11-25 22:30:29 +0700 | OpenVietGIS VPS Builder | `fix(gis): capture Space drag independently of editing tools` | Included in 064 baseline |
| 10 | `81d960b` | `81d960bbfad49268cd2322d01e6dda6d4117bc1d` | 2026-11-25 22:23:22 +0700 | OpenVietGIS VPS Builder | `fix(dev): temporary map pan, map layout export and feature inspector; refine chat workspace` | Included in 064 baseline |
| 11 | `fec3c85` | `fec3c85b502017b45fec42d093514fe54d58fb18` | 2026-11-25 22:15:55 +0700 | OpenVietGIS VPS Builder | `test(chat): narrow smoke test exception handling` | Included in 064 baseline |
| 12 | `b49cddf` | `b49cddff1af7168ac8ba137eb999a98eed0aa436` | 2026-11-25 22:15:45 +0700 | OpenVietGIS VPS Builder | `fix(chat): unify entry points and verify private realtime delivery` | Included in 064 baseline |
| 13 | `472a650` | `472a6507dba59caac7f08234273bee255742e4f5` | 2026-11-25 22:12:28 +0700 | OpenVietGIS VPS Builder | `feat(chat): connect organization rooms and contacts in dev UI` | Included in 064 baseline |

**Schema Parity Across `origin/main` (`1aa8dcb`), `main` (`3e78ac1`), and `064-rust-platform` (`babcdf9`)**:
`git ls-tree --name-only` diff confirms **zero Alembic migration changes** across all 13 commits between `1aa8dcb` and `3e78ac1` (all three refs contain the exact same 52 core migration `.py` files in `apps/api/migrations/versions/` and 11 module migration `.py` files in `modules/house_numbering/migrations/versions/`). The 13 local commits on `main` are frontend/UI/CAD/chat-client refinements on top of `1aa8dcb`.

---

## 2. Worktree & Unmerged Feature Branch Reconciliation Inventory

VPS1 registers **66 Git worktrees** (58 clean, **8 dirty**). Only dirty worktrees and branches with unmerged commits ahead of `main` (`3e78ac1`) require reconciliation tracking.

### 2.1 Dirty Worktrees on VPS1 (8 of 66)

| Worktree Path | Branch / Ref | Commit SHA | Dirty Items | Summary of Uncommitted Changes | Disposition Relative to Spec 064 |
| --- | --- | --- | --- | --- | --- |
| `/opt/openvietgis-rust-spatial-core` | `feat/rust-spatial-core` | `3e78ac1` | 5 | `M ROADMAP.md`, `M spatial_repository.py`, `M test_feature_persistence.py`, `?? apps/api/native_wkb/`, `?? specs/063-rust-native-wkb/` | **Excluded** — isolated Spec 063 WKB experiment; review separately before any reuse (`plan.md`). |
| `/opt/openvietgis-agent-worktrees/dev058` | `feat/061-chat-ui` | `ae7000c` | 5 | `M adapters/http/auth.py`, `M config.py`, `M Landing.tsx`, `M landing.css`, `?? vite.dev.config.ts` (actively runs `vite build --watch` to `/var/www/openvietgis-devui`) | **Reconcile separately** — `ae7000c` is 1 commit ahead of `main` (retires legacy `apps/web` SPA); dev-only uncommitted tweaks (`auth.py`/`config.py`/`Landing.tsx`) excluded from 064 until committed & reviewed on `main`. |
| `/opt/openvietgis-qgis-v1-final` | detached HEAD | `2d73f38` | 10 | `M managed_sync.py`, `M professional.py`, QGIS connector files, `?? snapshot_cache.py` | **Excluded / historical** — pre-`3e78ac1` QGIS v1 staging worktree; superseded by consolidated `main` (`3e78ac1`). |
| `/opt/openvietgis-qgis-web-combined` | detached HEAD | `dfa9587` | 10 | `M openvietgis_connector-1.0.0.zip*`, `M QgisBindingManager.tsx`, untracked `node_modules` & zip artifacts | **Excluded / historical** — pre-`3e78ac1` combined web/QGIS bundle build artifacts. |
| `/opt/openvietgis-v1-authz` | detached HEAD | `d352e3a` | 3 | `M managed_sync.py`, `M test_r029_session_auth.py`, `M specs/030-v1-unified-release/tasks.md` | **Excluded / historical** — Spec 030 authz hotfix scratchpad; superseded by `main`. |
| `/opt/openvietgis-v11` | `release/v1.0-unified` | `8d6a914` | 5 | `M ROADMAP.md`, `M managed_client.py`, `M plugin.py`, `?? specs/030-v1-unified-release/`, `?? test_qgis_v1_safety.py` | **Excluded / historical** — 286 commits behind `main`; archive/prune candidate after operator review. |
| `/opt/openvietgis-spatial-engine` | detached HEAD | `d43b1a5` | 5 | Untracked `node_modules` directories only (no source edits) | **Excluded / build artifact** — prune candidate to recover VPS1 disk. |
| `/opt/openvietgis-v8i-agent-worktrees/frontend` | `agentkit/frontend-v8i-20261123` | `1dc21fa` | 2 | Untracked `node_modules` directories only (no source edits) | **Excluded / build artifact** — prune candidate to recover VPS1 disk. |

### 2.2 Unmerged Local Feature Branches Ahead of `main` (`3e78ac1`)

Command: `git rev-list --left-right --count main...<branch>` across all `refs/heads/*` (`ahead > 0`).

| Branch | Commit SHA | Ahead / Behind `main` | Last Commit Date | Status | Disposition Relative to Spec 064 |
| --- | --- | --- | --- | --- | --- |
| `064-rust-platform` | `babcdf9` | +2 / -0 | 2026-11-25 23:37 +0700 | Clean (active worktree) | **In-scope (active Spec 064 branch)** |
| `feat/061-chat-ui` | `ae7000c` | +1 / -0 | 2026-11-25 23:40 +0700 | **Dirty (5 items)** in `dev058` | **Post-baseline delta** — retires legacy `apps/web` SPA & refines CAD/processing controls; reconcile explicitly if promoted to `main` before 064 scope freeze. |
| `spec/059-core-realtime-acceptance` | `e21e51a` | +17 / -101 | 2026-11-25 21:00 +0700 | Clean (`spec059`) | **Excluded / reference** — realtime visual acceptance suite; reuse test scenarios in T014/E17 without merging branch into 064 baseline. |
| `r048/native-delta-qualification` | `badd42e` | +1 / -209 | 2026-11-24 19:37 +0700 | Clean | **Excluded / historical** — CAD XY DXF qualification branch; verify whether CAD parity (E12) needs any delta. |
| `048-cad-gis-interoperability` | `448dbbd` | +6 / -281 | 2026-11-24 17:34 +0700 | Clean | **Excluded / historical** — R048 CAD interop branch (281 commits behind `main`). |
| `fix/landing-viewport-fit-20261124` | `cb75f85` | +2 / -281 | 2026-11-24 16:05 +0700 | Clean | **Excluded / superseded** — landing CSS iterations superseded by `main`. |
| `integrate/cesium-main-20261124` | `f788e5c` | +2 / -282 | 2026-11-24 14:57 +0700 | Clean | **Excluded / superseded** — Cesium integration tests superseded by `main`. |
| `feat/spec030-cesium-3d` | `1f8f853` | +1 / -280 | 2026-11-24 14:49 +0700 | Clean | **Excluded / superseded** — Cesium 3D feature already consolidated into `main`. |
| `feature/039-spatial-data-engine` | `4fe715a` | +1 / -280 | 2026-11-24 14:33 +0700 | No worktree | **Reference only** — 10M-point SQL benchmark docs; consult during T003/T012 benchmarking. |
| `qgis/v1.0-acceptance` | `f886514` | +2 / -297 | 2026-11-24 12:09 +0700 | No worktree | **Excluded / historical** — QGIS v1 acceptance branch; `main` has newer R029/042 commits. |
| `qa/spec030-connector-20261124` | `80beb06` | +1 / -297 | 2026-11-24 12:07 +0700 | Clean | **Excluded / historical** — QGIS connector QA branch. |
| `fix/mcp-autonomous-gis-policy` | `022541e` | +1 / -316 | 2026-11-24 11:48 +0700 | Clean | **Excluded / historical** — MCP approval safeguard draft (316 commits behind `main`). |
| `fix/qgis-permission-scope2` | `6f868ca` | +1 / -300 | 2026-11-24 11:27 +0700 | Clean | **Excluded / superseded** — superseded by migration `0042_qgis_pairing_scope` on `main`. |
| `ui/qgis-connector-simple` | `c3e5a06` | +1 / -301 | 2026-11-24 11:06 +0700 | Clean | **Excluded / historical** — QGIS UI simplification branch. |
| `fix/r029-qgis-permission-recovery` | `03ca5ec` | +1 / -302 | 2026-11-24 10:52 +0700 | Clean | **Excluded / superseded** — superseded by `main`. |
| `feat/landing-core-founder-poster-20261124` | `53ddfcd` | +1 / -330 | 2026-11-24 01:26 +0700 | No worktree | **Excluded / superseded** — legacy landing experiment. |
| `feat/landing-founders-motion-20261124` | `a8021bd` | +1 / -331 | 2026-11-24 00:38 +0700 | No worktree | **Excluded / superseded** — legacy landing experiment. |
| `feat/qgis-home-fidelity-20261123` | `7b29d95` | +1 / -334 | 2026-11-23 23:56 +0700 | No worktree | **Excluded / superseded** — legacy landing experiment. |
| `feat/qgis-landing-donate-20261123` | `54a59b7` | +1 / -340 | 2026-11-23 23:29 +0700 | No worktree | **Excluded / superseded** — legacy landing experiment. |
| `feature/028-windows-v8i-receiver` | `e03067c` | +7 / -393 | 2026-11-23 19:37 +0700 | Clean | **Excluded / out of 064 server scope** — Windows V8i desktop receiver pilot. |
| `feature/027-bidirectional-integration-sync` | `e3811b9` | +6 / -382 | 2026-11-23 19:23 +0700 | Clean (`[gone]`) | **Excluded / superseded** — R027 migrations `0039`–`0040` already on `main`. |
| `feature/025-microstation-v8i-bridge` | `c3752ee` | +13 / -408 | 2026-11-23 14:56 +0700 | Clean | **Excluded / historical** — MicroStation V8i bridge pilot (408 behind `main`). |
| `agentkit/*-v8i-20261123` (5 branches) | `b50f198`, `1dc21fa`, `fd9bbaf`, `17d57ef`, `7c1074b` | +2..+4 / -408 | 2026-11-23 14:40–14:48 +0700 | 4 clean, 1 dirty (`node_modules`) | **Excluded / historical** — temporary V8i multi-agent lane branches. |
| `backup/full-local-history-20261123` | `ab3a18c` | +9 / -409 | 2026-11-23 12:53 +0700 | No worktree | **Preserved archive ref** — do not delete or force-push. |
| `backup/main-before-cad-cleanup-20261123` | `a3697a4` | +4 / -409 | 2026-11-23 12:50 +0700 | No worktree | **Preserved archive ref** — do not delete or force-push. |

*(All other local branches — including `maintenance/032..047`, `feat/048-survey-terrain`, `feat/049-terrain-engineering`, `feat/unified-gis-cad`, `docs/readme-community-legal-20261124`, `release/v1.0-unified`, etc. — have `0` commits ahead of `main` `3e78ac1`.)*

---

## 3. Live PostgreSQL / PostGIS Schema Evidence (Read-Only)

Both running PostGIS containers use the exact same pinned image digest:
`postgis/postgis@sha256:fe440a27c6567aefa932e988962e5827bb2c29e4b97dac2ca5268f5eafe10262` (`PostgreSQL 18.7 (Debian 18.7-1.pgdg13+2) on x86_64-pc-linux-gnu, compiled by gcc (Debian 14.2.0-19) 14.2.0, 64-bit`).

Database roles on both containers:
- `openvietgis` — `Superuser, Create role, Create DB, Replication, Bypass RLS` (migrations / owner role)
- `openvietgis_runtime` — `No inheritance` (`NOSUPERUSER, NOBYPASSRLS`; application runtime role subject to `FORCE ROW LEVEL SECURITY`)

### 3.1 Cross-Instance Database Summary

| Container (Loopback Port) | Database | Consuming Live Service | PG / PostGIS Version | Core `alembic_version` | Module `alembic_version_house_numbering` | Base Tables (Schemas) | Views | RLS Tables (`FORCE RLS`) | Total RLS Policies | Registered `geometry_columns` | DB Size |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| **`hn019-preview-postgres`** (`127.0.0.1:54340`) | **`openvietgis_test`** *(Canonical live schema)* | `openvietgis-hn019-api` (`127.0.0.1:8019`, release `v2026.11.25-1aa8dcb-chat-rooms`) | PG `18.7` / PostGIS `3.7.0` (+ `postgis_topology 3.7.0`) | **`0043_chat_rooms`** *(HEAD)* | **`hn_0011_survey_signoffs`** *(HEAD)* | **112** (24 schemas) | **3** (`house_numbering.route_lines`, `public.geography_columns`, `public.geometry_columns`) | **102** (all forced) | **135** | **3** (`gis_access.rules.region` MULTIPOLYGON/4326; `house_numbering.route_lines.geometry` GEOMETRY/0; `spatial.reference_features.geometry` GEOMETRY/0) | `128 MB` |
| `dev-postgres-test-1` (`127.0.0.1:54330`) | `openvietgis_live` | `openvietgis-api` (`127.0.0.1:8000`, `/opt/openvietgis` @ `96897a2`) | PG `18.7` / PostGIS `3.7.0` | `0019_extension_history` | `hn_0005_building_lineage_plaque` | **67** (20 schemas) | **2** (`public.geography_columns`, `public.geometry_columns`) | **59** (all forced) | **61** | **1** (`spatial.reference_features.geometry` GEOMETRY/0) | `20 MB` |
| `dev-postgres-test-1` (`127.0.0.1:54330`) | `openvietgis_test` | Legacy dev/test DB (idle) | PG `18.7` / PostGIS `3.7.0` (+ `postgis_topology 3.7.0`) | `0019_extension_lifecycle` | `hn_0005_building_lineage_plaque` | **66** (21 schemas) | **2** (`public.geography_columns`, `public.geometry_columns`) | **56** (all forced) | **58** | **1** (`spatial.reference_features.geometry` GEOMETRY/0) | `293 MB` |

**Authoritative Schema Finding**: `hn019-preview-postgres` (`127.0.0.1:54340/openvietgis_test`) is the **only live database at full migration HEAD** (`0043_chat_rooms` + `hn_0011_survey_signoffs`, 112 base tables across 24 schemas, 102 forced-RLS tables, 135 policies). `dev-postgres-test-1` holds older R019-era schemas (`0019_extension_history` / `0019_extension_lifecycle`). All Spec 064 parity and staging-restore tests MUST target the `0043_chat_rooms` + `hn_0011_survey_signoffs` schema as canonical.

### 3.2 Schema Table & RLS Distribution (Canonical `hn019-preview-postgres/openvietgis_test`)

| Schema | Base Tables | RLS-Enabled Tables | Complete List of RLS-Enabled Tables (`FORCE ROW LEVEL SECURITY = true`) |
| --- | ---: | ---: | --- |
| `audit` | 1 | 1 | `audit_events` (2 policies) |
| `cad` | 2 | 2 | `conversions`, `document_versions` |
| `cartography` | 4 | 4 | `designer_layouts`, `layer_styles`, `print_exports`, `print_layouts` |
| `collaboration` | 16 | 16 | `activities`, `change_reviews`, `chat_attachments`, `chat_contact_requests`, `chat_contacts`, `chat_messages`, `chat_pins`, `chat_reactions`, `chat_room_members`, `chat_rooms`, `comment_mentions` (3), `feature_edit_leases` (4), `messages` (2), `review_comments`, `task_notification_reads` (2), `tasks` (3) |
| `editing` | 2 | 2 | `changeset_operations`, `changesets` |
| `extensions` | 10 | 10 | `activation_events`, `import_receipts`, `integration_events` (2), `manifests`, `marketplace_publishers` (2), `marketplace_submissions` (4), `partner_contributions`, `project_activations`, `record_revisions`, `records` |
| `external_sources` | 2 | 2 | `allowlisted_hosts`, `connections` |
| `field` | 2 | 2 | `offline_packages`, `sync_batches` |
| `gis_access` | 5 | 5 | `group_members`, `groups`, `reviews`, `rules`, `scoped_members` |
| `house_numbering` | 20 | 20 | `access_log`, `address_numbers`, `areas`, `assignments`, `building_lineage`, `buildings`, `documents`, `evidence`, `geometry_address_reviews`, `geometry_reviews`, `import_batches`, `previews`, `proposal_items`, `proposals`, `reconcile_cases`, `restricted_households`, `routes`, `rules`, `survey_signoffs`, `surveys` (+ view `route_lines`) |
| `identity` | 4 | 1 | `user_avatars` (2); *(3 non-RLS identity core tables)* |
| `integration_hub` | 7 | 7 | `binding_pairing_tokens` (4), `connector_bindings` (4), `desktop_commands`, `managed_sessions` (3), `project_event_counters`, `project_events` (2), `sync_conflicts` (3) |
| `maps` | 3 | 3 | `map_operations`, `map_revisions`, `project_maps` |
| `ops` | 1 | 1 | `organization_entitlements` |
| `org` | 6 | 5 | `invitations`, `organization_memberships` (2), `resource_grants`, `service_credentials`, `service_principals`; *(1 non-RLS root table `organizations`)* |
| `platform` | 1 | 0 | *(non-RLS platform bootstrap table)* |
| `processing` | 2 | 2 | `jobs`, `models` |
| `public` | 3 | 0 | `alembic_version`, `alembic_version_house_numbering`, `spatial_ref_sys` (+ PostGIS views) |
| `queries` | 1 | 1 | `saved_queries` |
| `raster` | 2 | 2 | `layers`, `tiles` |
| `spatial` | 10 | 10 | `catalog_changes` (2), `data_sources` (2), `layers` (2), `model_assets`, `model_placements`, `reference_features` (2), `scene_versions`, `source_fields` (2), `tileset_contents`, `tilesets` |
| `topology` | 2 | 0 | PostGIS `postgis_topology` extension tables (`topology`, `layer`) |
| `transfers` | 4 | 4 | `derived_artifacts`, `job_attempts`, `jobs`, `source_assets` |
| `workspace` | 2 | 2 | `projects`, `workspaces` |
| **Total** | **112** | **102** | **135 total RLS policies; all 102 RLS tables enforce `FORCE ROW LEVEL SECURITY`** |

---

## 4. Complete Alembic Migration Inventory (`3e78ac1` / `1aa8dcb` / `babcdf9`)

**Exact Measured File Count Reconciliation**:
- `apps/api/migrations/versions/`: **52** Alembic migration `.py` version files (linear head: `0043_chat_rooms`, after 5 consolidation merge revisions `0034`–`0038`). Including `apps/api/migrations/env.py`, `apps/api/migrations/script.py.mako`, and `apps/api/alembic.ini` yields **55** core Alembic artifacts in total, and **63** total migration `.py` version files across the monorepo when adding the **11** official `house_numbering` module migrations in `modules/house_numbering/migrations/versions/`.

### 4.1 Core Platform Migrations (`apps/api/migrations/versions/` — 52 files)

| # | Filename | Revision ID | `down_revision` |
| ---: | --- | --- | --- |
| 1 | `0001_foundation_sample.py` | `0001_foundation_sample` | `None` |
| 2 | `0002_identity_tenant_core.py` | `0002_identity_tenant_core` | `0001_foundation_sample` |
| 3 | `0003_tenant_rls.py` | `0003_tenant_rls` | `0002_identity_tenant_core` |
| 4 | `0003a_identity_unlink_security.py` | `0003a_identity_unlink_security` | `0003_tenant_rls` |
| 5 | `0004_machine_identity.py` | `0004_machine_identity` | `0003a_identity_unlink_security` |
| 6 | `0005_retire_foundation_sample.py` | `0005_retire_foundation_sample` | `0004_machine_identity` |
| 7 | `0006_spatial_catalog.py` | `0006_spatial_catalog` | `0005_retire_foundation_sample` |
| 8 | `0007_map_workspace.py` | `0007_map_workspace` | `0006_spatial_catalog` |
| 9 | `0008_import_export.py` | `0008_import_export` | `0007_map_workspace` |
| 10 | `0009_editing_changesets.py` | `0009_editing_changesets` | `0008_import_export` |
| 11 | `0010_saved_queries.py` | `0010_saved_queries` | `0009_editing_changesets` |
| 12 | `0011_processing_jobs.py` | `0011_processing_jobs` | `0010_saved_queries` |
| 13 | `0012_cartography_layouts.py` | `0012_cartography_layouts` | `0011_processing_jobs` |
| 14 | `0013_external_sources.py` | `0013_external_sources` | `0012_cartography_layouts` |
| 15 | `0014_team_collaboration.py` | `0014_team_collaboration` | `0013_external_sources` |
| 16 | `0015_cad_precision.py` | `0015_cad_precision` | `0014_team_collaboration` |
| 17 | `0016_field_offline.py` | `0016_field_offline` | `0015_cad_precision` |
| 18 | `0017_extension_platform.py` | `0017_extension_platform` | `0016_field_offline` |
| 19 | `0018_production_ops.py` | `0018_production_ops` | `0017_extension_platform` |
| 20 | `0019_extension_history.py` | `0019_extension_history` | `0018_production_ops` |
| 21 | `0020_manual_layers.py` | `0020_manual_layers` | `0019_extension_history` |
| 22 | `0021_professional_tools.py` | `0021_professional_tools` | `0020_manual_layers` |
| 23 | `0022_schema_change_evidence.py` | `0022_schema_change_evidence` | `0021_professional_tools` |
| 24 | `0023_user_avatar.py` | `0023_user_avatar` | `0022_schema_change_evidence` |
| 25 | `0023_gis_access.py` | `0023_gis_access` | `0023_user_avatar` |
| 26 | `0024_raster_layers.py` | `0024_raster_layers` | `0023_gis_access` |
| 27 | `0025_processing_models.py` | `0025_processing_models` | `0024_raster_layers` |
| 28 | `0026_layout_designer.py` | `0026_layout_designer` | `0025_processing_models` |
| 29 | `0027_partner_platform.py` | `0027_partner_platform` | `0026_layout_designer` |
| 30 | `0028_marketplace_developers.py` | `0028_marketplace_developers` | `0027_partner_platform` |
| 31 | `0029_cad_documents.py` | `0029_cad_documents` | `0028_marketplace_developers` |
| 32 | `0029_team_chat_tasks.py` | `0029_team_chat_tasks` | `0028_marketplace_developers` |
| 33 | `0030_collaboration_realtime.py` | `0030_collaboration_realtime` | `0029_team_chat_tasks` |
| 34 | `0030_model_vault.py` | `0030_model_vault` | `0029_cad_documents` |
| 35 | `0031_saved_scenes.py` | `0031_saved_scenes` | `0030_model_vault` |
| 36 | `0031_spatial_presence_events.py` | `0031_spatial_presence_events` | `0030_collaboration_realtime` |
| 37 | `0032_feature_edit_leases.py` | `0032_feature_edit_leases` | `0031_spatial_presence_events` |
| 38 | `0032_tilesets.py` | `0032_tilesets` | `0031_saved_scenes` |
| 39 | `0032_unified_spatial_collaboration.py` | `0032_unified_collaboration` | `("0029_cad_documents", "0031_spatial_presence_events")` |
| 40 | `0033_feature_comments.py` | `0033_feature_comments` | `0032_feature_edit_leases` |
| 41 | `0033_tileset_provenance.py` | `0033_tileset_provenance` | `0032_tilesets` |
| 42 | `0034_consolidated_heads.py` | `0034_consolidated_heads` | `("0030_model_vault", "0033_feature_comments", "0032_unified_collaboration")` |
| 43 | `0034_tile_feature_index.py` | `0034_tile_feature_index` | `0033_tileset_provenance` |
| 44 | `0035_consolidated_scenes.py` | `0035_consolidated_scenes` | `("0034_consolidated_heads", "0031_saved_scenes")` |
| 45 | `0036_consolidated_tiles.py` | `0036_consolidated_tiles` | `("0035_consolidated_scenes", "0032_tilesets")` |
| 46 | `0037_consolidated_generation.py` | `0037_consolidated_generation` | `("0036_consolidated_tiles", "0033_tileset_provenance")` |
| 47 | `0038_consolidated_feature_index.py` | `0038_consolidated_feature_index` | `("0037_consolidated_generation", "0034_tile_feature_index")` |
| 48 | `0039_r027_connector_bindings.py` | `0039_r027_connector_bindings` | `0038_consolidated_feature_index` |
| 49 | `0040_r027_pairing_and_conflicts.py` | `0040_r027_pairing_and_conflicts` | `0039_r027_connector_bindings` |
| 50 | `0041_r029_managed_realtime_client.py` | `0041_r029_managed_realtime` | `0040_r027_pairing_and_conflicts` |
| 51 | `0042_qgis_pairing_scope.py` | `0042_qgis_pairing_scope` | `0041_r029_managed_realtime` |
| 52 | `0043_chat_rooms.py` | `0043_chat_rooms` | `0042_qgis_pairing_scope` |

### 4.2 Official Module Migrations (`modules/house_numbering/migrations/versions/` — 11 files)

| # | Filename | Revision ID | `down_revision` |
| ---: | --- | --- | --- |
| 1 | `0001_house_numbering.py` | `hn_0001_house_numbering` | `None` |
| 2 | `0002_private_access_gate.py` | `hn_0002_private_access_gate` | `hn_0001_house_numbering` |
| 3 | `0003_performance_indexes.py` | `hn_0003_performance_indexes` | `hn_0002_private_access_gate` |
| 4 | `0004_survey_gps_delegation.py` | `hn_0004_survey_gps_delegation` | `hn_0003_performance_indexes` |
| 5 | `0005_building_lineage_plaque.py` | `hn_0005_building_lineage_plaque` | `hn_0004_survey_gps_delegation` |
| 6 | `0006_route_segments.py` | `hn_0006_route_segments` | `hn_0005_building_lineage_plaque` |
| 7 | `0007_route_lines_fast_path.py` | `hn_0007_route_lines_fast_path` | `hn_0006_route_segments` |
| 8 | `0008_reviewed_route_aliases.py` | `hn_0008_reviewed_route_aliases` | `hn_0007_route_lines_fast_path` |
| 9 | `0009_geometry_review.py` | `hn_0009_geometry_review` | `hn_0008_reviewed_route_aliases` |
| 10 | `0010_address_verification.py` | `hn_0010_address_verification` | `hn_0009_geometry_review` |
| 11 | `0011_survey_signoffs.py` | `hn_0011_survey_signoffs` | `hn_0010_address_verification` |

---

## 5. Verified Rollback Tag & Recovery Anchors

A lightweight local Git rollback tag was created and verified without modifying any remote or branch history:

```text
$ git tag rollback/064-baseline-3e78ac1 3e78ac1
$ git show-ref --tags
3e78ac142acfc050ec22d30da5c44ad0e6b8459a refs/tags/rollback/064-baseline-3e78ac1
```

| Rollback Anchor | Exact Identifier / Path | Verification |
| --- | --- | --- |
| Local Git rollback tag | `refs/tags/rollback/064-baseline-3e78ac1` -> `3e78ac142acfc050ec22d30da5c44ad0e6b8459a` | Verified via `git show-ref --tags` |
| Remote GitHub `main` anchor | `refs/remotes/origin/main` -> `1aa8dcb2fa76bbedfe8a08a33da3d7bd5fa9fddc` | Verified via `git rev-parse origin/main` |
| Live Python API release directory | `/opt/openvietgis-releases/v2026.11.25-1aa8dcb-chat-rooms` | Active in `/etc/systemd/system/openvietgis-hn019-api.service.d/90-ovg-release.conf` |
| Live Python API venv | `/opt/openvietgis-releases/v2026.11.24-bda9703-geopackage/apps/api/.venv/bin/python` | Active `ExecStart` in `openvietgis-hn019-api.service` |
| Canonical DB schema revisions | Core `0043_chat_rooms` + Module `hn_0011_survey_signoffs` on `hn019-preview-postgres:54340/openvietgis_test` | Additive-only migrations required until SC-006 rollback rehearsal passes |
| Staging backup/restore & rollback scripts | `infra/rust-platform/staging-backup-restore.sh`, `infra/rust-platform/rollback-rehearsal.sh` | Rehearsed in T004 against isolated temporary DB |

