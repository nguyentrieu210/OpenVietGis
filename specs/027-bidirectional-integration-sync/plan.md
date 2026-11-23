# Implementation Plan: R027 — Bidirectional Integration Sync

**Branch**: `feature/027-bidirectional-integration-sync` | **Date**: 2026-11-23 | **Spec**: [spec.md](spec.md)

**Status**: Planning baseline only. The creation of this document does not deploy or implement a connector.

## Summary

Build independently verifiable, project-scoped bidirectional QGIS integration on top of R026 discovery, R003 identity/CRS/revisions and R006 atomic changesets. Add secure connector binding, deterministic diff/preview, external identity map, durable outbox/inbox, explicit conflict resolution and credential-aware worker execution. Keep auto-sync, external PostGIS agent and desktop CAD providers behind separate release gates.

## Technical Context

- **Backend**: existing FastAPI/Python API in `apps/api`, existing PostgreSQL/PostGIS and migration conventions; no new database choice.
- **Frontend**: existing `apps/web-next` Integration Hub project UI; typed `packages/client-sdk`.
- **Agent**: narrow QGIS desktop client/plugin over HTTPS; desktop credentials must live in OS-secured storage.
- **Testing**: existing pytest unit/integration, real PostGIS + RLS tests, typed SDK tests, browser E2E and genuine QGIS desktop acceptance; network partition/duplicate delivery fault injection.
- **Scale/budget**: benchmark first 1,000 features per run; enforce bounded pages, processing time, memory, rate limits and per-binding quotas. Performance target finalized from measured baseline, not promised in advance.
- **Constraints**: R002/R003/R006/R010/R011/R013/R026 APIs, default deny writes, conditional sync, no direct DB access or cloud COM/DCOM.

## Constitution Check

- **I Platform-first**: connector transport is core integration infrastructure; product-specific cadastral semantics remain in modules. PASS design.
- **II Interoperability**: open GeoPackage/GeoJSON/API escape hatch preserved; provider-specific adapters optional. PASS design.
- **III Tenant isolation**: every operation and worker rechecks server ACL and RLS; never trust UI policy. **GATE** integration proof pending.
- **IV CRS**: no inferred CRS, validated transform, geometry loss warnings. **GATE** control-point proof pending.
- **V Versioned changes**: R006 for writes; immutable origin, audit, outbox and conflict trail. **GATE** ledger implementation pending.
- **VI Capabilities**: adapter lifecycle and operation flags truthful. PASS design.
- **VII Engine agnostic**: no vendor binaries in Core. PASS design.
- **VIII Workflows before parity**: QGIS two-way end-to-end before multi-provider marketing. PASS design.
- **Security/recovery**: token rotation, SSRF guard, expiry, rollback and drill before production. **GATE** pending.
- **Architecture change ADR**: durable event journal, external identity mapping and agent trust model MUST receive ADR review before implementation.

## Milestones and acceptance gates

**Phase 0 — Baseline reconciliation**
- Verify exact R026 state on `main` and production; distinguish read-only catalog, GeoPackage two-step candidate and live functionality.
- Measure existing R006 changeset/revert and R013 outbox behavior.
- Record provider capability/credential matrix and real QGIS versions.

**Phase 1 — Manual bilateral MVP (P1)**
- Design binding registry, scoped authority, external ID map, schema/CRS mapping, diff and conflict model.
- Read a QGIS dataset through verified plugin/agent and preview remote/local diffs.
- Explicitly approve non-conflicting creates/updates and use R006 to commit to OVG.
- Pull authorized OVG change out to QGIS through conditional local apply and durable ack.
- Prove no overwrite, echo, revoked replay or hidden-field disclosure.

**Phase 2 — Durable automation (P2; independent security gate)**
- Transactional event/outbox + consumer checkpoint; resilient retries and reconciliation.
- Start with safe allowed create/update only. Conflict and delete always held for review.
- Operator dashboard, cadence, pause/resume, queue metrics, retention and alerting.

**Phase 3 — Provider expansions (P3; separate acceptance)**
- External PostGIS **agent** via secured API (no direct OVG DB credentials); ArcGIS/AutoCAD/MapInfo/V8i as separately licensed/tested adapter candidates.
- Provider-specific correctness tests and explicit capability downgrade to export-only where write semantics cannot be guaranteed.

## Project Structure (planned, exact names subject to ADR)

```text
specs/027-bidirectional-integration-sync/
  spec.md
  research.md
  plan.md
  data-model.md
  contracts/sync-api.yaml
  tasks.md
  quickstart.md
apps/api/
  src/             # extend existing domains/routes after verifying actual boundaries
  tests/           # real PostGIS/RLS and fault injection
apps/web-next/src/ # extend existing Integration Hub
packages/client-sdk/src/ # typed binding/preview/run client
integrations/qgis_plugin/ # extend verified QGIS agent only
```

**Structure decision**: integrate through existing Core contracts. Do not introduce separate monolith, duplicate changeset engine or plugin privilege bypass.

## Rollout, rollback and convergence

- Start in isolated feature branch; never write sample sync data into production. Use synthetic tenant/project fixtures.
- Feature flag per tenant and binding; binding disabled by default; deploy first to isolated staging with exact image SHA.
- Preserve database rollback/backups and connector journal; tested migration reversal is mandatory before activation.
- On incident, pause bindings and revoke credentials, stop workers, preserve snapshots/operation journal, revert new service release without erasing external acknowledgments.
- Convergence requires mapping each FR and SC to tests/evidence, plus operator signoff and independent review; neither file creation nor a green unit suite is completion.

## Risks / deferred decisions

- Missing official QGIS native edit hooks, unstable external feature identity, read-only providers and partner license limitations are blocking capability constraints, not things to paper over in UI.
- Automatic deletion and general bidirectional provider rollout remain OFF until explicit policies and isolated recovery tests prove safety.
