# Proposed interface contract (not current deployed API)
R014 generic endpoints already register/list manifests, activate/list project activations, create/list records. All additional contracts require verified prerequisite; names below logical operations, not existing routes. Module cannot bypass Core authorization or directly modify Core-owned data.

Every operation resolves project membership and activation server-side, verifies capability, namespace permission, ownership and revisions. Scope derives from authenticated context, not untrusted payload. Invalid data -> actionable error; stale/duplicate -> conflict; unauthorized -> denied; no sensitive fields in errors.

| Operation | Input | Output / invariants |
|---|---|---|
| Module lifecycle | project, manifest/version, enable/disable | state; records preserved; inactive reject |
| Area/route/building edit | scope, spatial feature reference, expected revision, change | validated revisions and history; route direction locks |
| Inspect | point + explicit CRS, scope/policy | existing, parcel, route/side/measure, evidence, neighbors, suggestions with reasons; read only |
| Preview assignment | target IDs, rule/version, expected revisions | candidate old/new diff, conflict/warnings, token; no mutation |
| Apply | token, idempotency key, expected version | atomic changes or entire failure; server revalidation |
| Proposal | items, rule, status, comment, expected revision | state and audit; approved unique, authority gate |
| Stage/commit import | authorized source, mapping, CRS, key | counts, conflicts, accepted; replay deterministic |
| Reconcile | scope, source revision | grouped cases, unmatched; resolution requires evidence |
| Survey sync | client UUID, geo, attachment hashes, actor | idempotent result, conflict, authorized delegation |
| Search/progress | scope, filters, pagination | bounded records/aggregate, PII separate |
| Inspection/print | building/point, verified template, rule, approved number | artifact, warnings, issue ledger |
| Export | scope, layers/fields/CRS, standard or authorized-original | job/artifact manifest; sensitive export purpose/audit |

Browser: project module workspace contribution with area tree + map + attribute table, actions gated by permission; mobile survey queue visible; inspect/preview read-only; clear disabled/error state. No direct generic CRUD over protected domain records.
