# SPEC 075 — Universal Plugin Platform & Marketplace
Branch: `075-universal-plugin-ecosystem` | Created: 2026-11-29 | Status: Wave 0 implementation in progress; acceptance gates open
Baseline: a4ae85a5e3d956de1870dcf65ff3fb21da7f0367
Parent: [R014](../../ROADMAP.md#r014), R002–R013, R015; extend existing AI/MCP, Rust, mobile SPEC 073 and desktop SPEC 074 contracts, subject to actual inventory.
## Scope
150 named capability packages in 15 groups, Wave 0 shared platform and Waves 1–15 product delivery. Existing implementations are reused, extended or wrapped, never duplicated. Catalog IDs are immutable; names are display labels, not trademark claims. No guarantee of full third-party product parity.
## Independently testable user stories
- US1 (P0): A GIS user opens a clean workspace with no domain plugins, installs an eligible signed plugin, opens a contextual workspace, runs its real command, exports results, disables it and retains authorized data.
- US2 (P0): An administrator grants minimal project permissions, rejects an untrusted publisher, revokes a release and observes all active sessions/jobs/leases follow policy.
- US3 (P0): A developer uses existing SDK/registry, builds a sample, validates manifest, signs, submits for review and installs on two supported hosts without Core domain imports.
- US4 (P0): A user edits from mobile offline, reconnects, resolves stale revisions and sees same IDs on Web/Desktop without duplicate mutations.
- US5 (P1): A designer builds a typed form, dashboard and processing DAG and publishes an immutable declarative app with auditable permission checks.
- US6 (P1): A tenant buys or receives entitlement, activates a compatible plugin and cancels/refunds through audited lifecycle; no billing status grants data permission.
- US7 (P1): A domain operator completes the workflow and oracle in the catalog for every selected package; unsupported capability is explicitly unavailable.
- US8 (P1): A self-host administrator mirrors signed releases, upgrades and rolls back on an isolated environment, with data and licenses portable.
## Requirements
- FR-001 Preserve canonical Registry, existing manifest and v1 signatures; inventory actual routes, schemas, commands, SDKs and existing PLG matches before coding.
- FR-002 Stable plugin ID, publisher namespace, immutable semver release and digest; separate draft/review/approved/published/revoked states.
- FR-003 Signed release and contribution envelopes compatible with SPEC 073; verify canonical bytes, signature, all asset hashes, expiry and monotonic epoch.
- FR-004 Resolve dependency DAG, host/API semver and capability intersection; reject cycles, unsupported dependencies and downgrade attacks.
- FR-005 Trusted host brokers every command with actor, org, project, resource and revision scope; permissions evaluated server-side at invocation and job execution.
- FR-006 Declarative UI workspace/tool/form/dashboard slots; no persistent forced domain UI; accessible vi/en, command search, lifecycle disposal.
- FR-007 Default-deny runtime; first-party compiled adapters, declarative partner packages; WASM/WASI opt-in separately qualified, no unrestricted native code, Python in isolated server worker only.
- FR-008 Quotas CPU/memory/time/output/concurrency/network; deny arbitrary file, device, credential and raw DB access; egress allowlist, cancellation and cleanup.
- FR-009 Reuse Core GIS commands, jobs, schemas, rendering, change history, audit, spatial CRS and sync; no domain imports in Core.
- FR-010 Data schema ownership and additive tenant-scoped migrations with backup/rollback strategy; disable/uninstall never deletes user records; explicit authorized deletion separate.
- FR-011 Lifecycle install, activate, suspend, upgrade, rollback, disable, uninstall, revoke; concurrency-safe idempotency and immutable audit.
- FR-012 Marketplace search/filter, compatibility, descriptions, permissions, publisher, changelog, reviews, signed artifacts, release channels and private catalogs.
- FR-013 Developer portal, publisher identity, key rotation, package validation, review queue, license/SBOM and vulnerability policy; signed operator approval separate from publisher.
- FR-014 Entitlement independent of authorization; free, paid, trial, subscription and enterprise grants; expiry/revocation and bounded offline policy.
- FR-015 Hosted checkout adapter, verified webhook, idempotent ledger, refunds, payouts and reconciliation; sandbox first, no card data in Core, legal/tax/store approval gates.
- FR-016 Web/Mobile/Desktop profiles with same IDs and command semantics; mobile declarative only unless approved native release, platform limitations visible.
- FR-017 SDK Rust/TS/Dart/Python/WASM/REST/Realtime/AI/declarative plus native bridge generated from canonical contracts; versioning and examples; no duplicate public SDK.
- FR-018 AI tool calls typed, scoped, budgeted, audited; explicit human approval for material writes/publish; untrusted document/imagery text cannot change authority.
- FR-019 No-code typed DAG validation, deterministic replay where possible, versioned forms/rules and bounded execution; no eval.
- FR-020 Every catalog entry delivers real workflow, validation, persistence/output, error handling and domain oracle; shared algorithms only once.
- FR-021 Licensed external providers, data/CRS provenance, attribution and credential vault; no scraping/bypass or unqualified legal/survey/engineering claims.
- FR-022 Safe observability, opt-in metrics, no raw PII/secrets; independent audit and release profiles, rollback rehearsal and source/license compliance.
- FR-023 Commercial paid capabilities do not change AGPL Core rights; component/license review before distribution, no arbitrary Core paywall.
- FR-024 Existing house numbering and QGIS integration preserve IDs, data and signatures; regression tests on exact integrated baseline.
- FR-025 Public maps, location, cadastral and sensor data enforce consent, retention, review and redaction; safety-critical industrial integrations read-only by default.
## Success criteria (targets, not measured claims)
- SC-001 Clean Core no plugin dependency; two reference verticals install/disable and export data with no regression.
- SC-002 150/150 PLG entries mapped to executable domain acceptance plus negative security and lifecycle checks; no placeholder passes.
- SC-003 2 orgs × 2 projects × 3 roles across web/API/jobs/cache/offline: zero unauthorized operations.
- SC-004 100 repeated/reordered install, webhook, mutation and sync cases: no duplicate side effects; rollback and key revocation demonstrated.
- SC-005 100 concurrent catalog readers with 1,000 releases: p95 metadata search <=300ms warm on documented 4 vCPU/8 GiB reference, excluding WAN; plugin metadata activation overhead p95 <=200ms excluding workload/download. Measure p50/p95/p99 and error rate.
- SC-006 SDK sample from documented environment <=30min; contract compatibility golden vectors on Rust/TS/Dart; two host E2E workflows, real-device mobile qualification per 073.
- SC-007 all published algorithms pass independently defined unit/tolerance/fixture oracles; AI model metrics set before qualification; scientific outputs carry limitations.
- SC-008 sandbox, tenant isolation, signed assets, supply-chain, revocation, migration and recovery tested; zero unresolved critical/high security or correctness defects.
- SC-009 commercial profile reconciles simulated payment, entitlement, refund, payout and webhook replay; production payment and legal approvals independently evidenced.
- SC-010 exact merged candidate with full regression, real environment evidence, independent review and capability-specific release matrix. No automatic live deploy.
## Non-goals
Second backend/registry, full desktop parity by assertion, unrestricted partner code, remote control of industrial equipment, official legal certification, mandatory accounts for independent local files, guaranteed AI/survey accuracy. Advanced scientific or paid profiles stay unavailable until qualified, but remain tracked open scope.
## Artifacts
[Plan](plan.md), [Tasks](tasks.md), [Catalog](plugin-catalog.md), [Execution](execution-matrix.md), [Acceptance](acceptance-matrix.md), [Data model](data-model.md), [Research](research.md), [Quickstart](quickstart.md), [Contracts](contracts/README.md), [Review](review-resolution.md).
