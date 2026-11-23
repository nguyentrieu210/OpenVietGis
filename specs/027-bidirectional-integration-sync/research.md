# Research & Design Decisions — R027

**Status**: Spec Kit Phase 0 research baseline (not implementation evidence).  
**Date**: 2026-11-23

## Existing foundations to reuse

| Existing capability | Reuse in R027 | Gap to close |
| --- | --- | --- |
| R026 Integration Hub | Adapter identity, catalog, policy preflight, read-only exports, two-step GeoPackage sync preview/push candidate | Persistent binding and true external two-way delivery |
| R006 changesets | Canonical, auditable OpenVietGIS writes; revision pins, revert | External receiver acknowledgment and changes crossing systems |
| R003 spatial catalog | Source/layer/feature IDs, schema/content revisions, explicit CRS | External IDs and map of heterogeneous field schemas |
| R002 + GIS ACL | Tenant, principal, feature/field/region enforcement | Scoped service permissions at every queue execution |
| R011 collaboration/reviews | Human review and activity | Connector conflict queues and sync approval UX |
| R013 offline sync | Existing outbox/idempotency concepts | Separate cross-product durable event journal and gap handling |
| R010 external sources | Host allowlist, SSRF guard | Outbound writing allowed only through independently verified connector |
| R014 Plugin SDK | Sandboxed UI extension | No direct CRUD through partner iframe; external agent needed |

## Research questions requiring explicit answers during implementation

1. **QGIS plugin protocol**: What is the actual plugin install/auth channel and stable feature-ID mechanism for chosen QGIS versions? Can local edits be detected without unsaved state corruption?
2. **OpenVietGIS event source**: Is R011 activity a complete immutable log? If not, implement a transactional outbox bound to R006 committed changes, not best-effort polling of a projection.
3. **Remote revisions**: Which providers guarantee ETags/row versions/transactions? Mark non-conditional providers snapshot-only or require agent-side immutable journal with compare-and-swap.
4. **Credential authorization**: Does current service principal permission ceiling support exact authorized feature writes? If not, a dedicated sync credential scope + human approval is a prerequisite, not a shortcut.
5. **Feature identity**: How to preserve split/merge and tombstones across QGIS edits and external PostGIS primary keys?
6. **CRS and geometry**: What reversible transform tolerances are acceptable for EPSG:4756, EPSG:5899 and supported CAD primitives? Prove with control points.
7. **GDPR/privacy equivalent and Vietnamese cadastral data**: Identify legal/organizational approval for sharing personal or land records before enabling cross-system export.
8. **Operational recovery**: Retention of event journal, cursors, payload digests, encrypted secrets, and backup restore for failover; explicit retention policy before enabling.
9. **UI semantics**: Are `bidirectional_auto` and delete operations separate elevated approvals with expiration? Default yes.

## Decision log (proposed; subject to ADR)

- **D1**: Never expose internal shared PostGIS to desktop clients. All OVG mutations go through authenticated scoped HTTPS + R006.
- **D2**: Prefer a remote agent/plugin with a durable inbox/outbox; use event-driven notifications where verified, safe polling otherwise, and periodic reconciliation to detect gaps.
- **D3**: Use two-stage `preview -> approve/commit` for first live version. No automatic destructive mutations.
- **D4**: Revisions and external IDs define conflicts; timestamps are diagnostic only.
- **D5**: Each connector reports actual capability (`verified`, `read_only`, `candidate`, `offline`), never inferred from catalog labels.
- **D6**: Commit final protocol, outbox and conflict ownership decisions to an ADR before coding the durable execution engine.

## Risks / mitigations

- **Cross-tenant leakage**: RLS plus feature/field ACL and service principal ceiling verified on every worker execution and replay.
- **Silent corruption**: explicit CRS + precision/geometry validation + diff preview + reversible history.
- **Event loss**: transactional outbox, cursor gap detection, reconciliation and retention monitoring.
- **Duplicate/echo edits**: idempotency IDs, origin chain and conditional revisions.
- **Secret theft**: managed encryption/rotation, agent OS keychain, never client-side persistent token files.
- **False product claims**: lifecycle state gates and live verification per adapter; release artifact IDs and evidence.
