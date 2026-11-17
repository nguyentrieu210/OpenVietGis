# Cross-spec contract gates (R002–R015)

Status: integration review findings and approved technical corrections; not accepted implementation contracts. The owning feature Plan/ADR and evidence must converge before consuming them. No new product scope.
Baseline: verified main ea6db79; individual draft branches documented in [coordination ledger](agent-coordination.md).

| Gate owner | Shared invariant to freeze | Consumers |
|---|---|---|
| R002 Identity | Actor and tenant context explicit and current on every protected transaction; requested service actors include exact nonsecret credential identity; reauthorize on submit, execute, publish, replay and fetch. R002 handoff: [live audit](r002-live-audit-handoff.md). | R003–R015 |
| R003 Spatial catalog | Distinct stable source/feature/field/schema IDs; distinguish source-content revision, catalog metadata revision, schema revision, map configuration revision. CRS axis convention and authority-approved transform/control evidence; no arbitrary automatic inference. Capability is not permission. | R004–R015 |
| R004 Map workspace | Renderer-neutral ordered map-layer references versus catalog layer/source lifecycle. Bounded viewport and feature detail, permission+capability gating, stable revision-pinned selections. Shared config save CAS, evidence of actual changes, reliable lost-ack reconciliation; personal camera separate. | R005–R010, R011–R014 |
| R005 Import/export and jobs | One durable job contract, shared attempts/idempotency/request hash, cancellation vs publication authority and exact revision manifest; immutable object bytes/checksum with DB-atomic ready reference and orphan reconciliation; no distributed atomicity assumption. Reauthorize artifact access each time; no irrevocable direct URL for revocable data. Whole-source export until explicit subset gate. | R006–R015 |
| R006 ChangeSet | One-source atomic change group, expected source revision and before/after evidence, bounded operation IDs/fingerprints and current permission. Conflict instead of silent rewrite; accepted replay and lost acknowledgement recovery. | R007/R008/R011–R015 |
| R007 Queries | Typed stable-field expressions, revision-pinned target and reference sources, bounded actual result manifest; no misleading partial/exact selection. Subset export not advertised until accepted R005 extension. | R008/R009/R011/R012/R014 |
| R008 Processing | Reuse R005 job/artifact contract; current authorized access, frozen input semantics, new output source only, no silent parameter/schema omissions. Reject unsupported execution classes. | R014 modules when enabled |
| R009/R010 Render/external | Saved legend/style may expose protected values; authorize metadata. Distinguish external config revision from content; export only licensed immutable reproducible content, never re-fetch latest on retry; origin-specific credentials, verified redirect/IP/TLS, scope cache and recheck revocation. | R009/R010/R013/R015 |
| R011/R012/R013 Collaboration/CAD/offline | Review tied to exact revision and explicit authority; CAD handles drawing-local, snap reference revision/current authorization; server-issued canonical offline package/allowed scope and same-source atomic batch, replay-safe sync with no quiet base revision rewrite. | R014/R015 |
| R014/R015 Module/production | Installation operator authority separate tenant admin; new permission default deny; portable data independent of module code, recoverable versioned migrations. Restore cannot resurrect revoked grants or credentials; coherent DB/identity/artifact/job checkpoint, pinned release, tenant fairness. Billing/SDK separate gates. | Release |

## Independent evidence before implementation acceptance

- Actual runtime non-owner DB role; two orgs/projects with overlapping IDs and differentiated access; direct-ID substitution, revocation and stale response, connection reuse.
- Deterministic reference datasets with licensed independent CRS definition/control pairs and exact accepted tolerances; synthetic fixtures supplement, not replace, authoritative validation.
- Interrupted operations before and after publication, lost acknowledgement, cancel-versus-publish race, same/different operation fingerprint retries, object/database inconsistencies, restore and role revocation.
- Data/selection/export round-trip validated with independent GIS tools, explicit null/type/schema/geometry/CRS/loss matrix; no hidden omissions. Layer/map configuration versus source-content invalidation explicitly tested.
- Network fixtures for service origin/credential redirects, DNS/IP/TLS/SSRF, caching rights, protected metadata, unsupported external snapshot.
- Concurrent same-source edits, multi-operation offline packages, package/actor/scope tampering, CAD reference revision changes and portable module data on disable.
- Frozen release commit/profile, M1 and M3 end-to-end workflow, upgrade/recovery, resource fairness and measured recovery windows. Benchmark proposals remain unaccepted until evidence/profile approved.

## Integration ownership

Do not merge R003/R004 old-baseline branches wholesale; they include old R002/ROADMAP history. At R002 convergence, transplant only each feature's spec artifacts onto current verified main and resolve shared traceability separately. R005–R015 drafted from ea6db79; synchronize main only at a clean checkpoint. No concurrent edits to common contracts or active dirty worktrees.
