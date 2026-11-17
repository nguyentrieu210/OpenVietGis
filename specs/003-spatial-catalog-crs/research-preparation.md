# R003 Research and Grill Preparation

**Status**: draft discussion aid; not a plan, not an implementation contract, not an accepted ADR.

**Base**: `98a65eb` on branch 003-spatial-catalog-crs; roadmap commit 0378506; R001 convergence commit e41639c.

## Verified repo baseline

| Source | Status | R003 consequence |
| --- | --- | --- |
| Constitution I–VIII | Ratified | Core/module isolation, explicit CRS, tenant safety, history and real integration checks |
| ROADMAP R003 | Planned prerequisite R002 | Reference source metadata and read; exclude full import and editor |
| ADR-005 | Accepted | Structured data in PostgreSQL/PostGIS; no arbitrary large blobs in relational rows |
| ADR-006 | Accepted | Layer is not source/table; explicit capabilities; engine-independent core |
| ADR-007 | Accepted | Attributable changes and revisions from first mutation; short transactions |
| ADR-010 | Accepted | SourceAsset, DerivedArtifact and Attachment have distinct semantics |
| ADR-014 | Accepted architecture; runtime pending | Explicit tenant, actor, scoped requests, RLS defense-in-depth |
| R002 permission registry 0.2 | Planning baseline | project.read/edit/manage; scoped grants, service ceiling, live revocation |
| R002 data model | Planning baseline | Organization direct ownership; optional Workspace; Project lifecycle |
| R002 implementation | Not converged at base | Block R003 implementation, schema migrations and final contract until reconciliation |

One safe integration direction: R003 evaluates its own resource authorization at Project scope while reusing R002 actor/tenant context, rather than inventing a parallel authority system. No permission registry changes are accepted here.

## Three decisions to clarify before plan

| ID | Question | A — recommended first slice | B — larger alternative |
| --- | --- | --- | --- |
| Q1 | Ownership/reuse model | Project-scoped source and Layer; multiple Layers can reference the same source within Project. Organization always explicit. | Organization-level source catalog with cross-Project reuse; introduces independent source grants, discovery and lifecycle dependency risk. |
| Q2 | CRS support | Explicit finite matrix: WGS84, Web Mercator and chosen VN-2000 profile(s) verified by actual independent controls; reject unknown/unvalidated cases. | User-defined CRS definitions and transformation configuration, increasing correctness and operational risks. |
| Q3 | Reference source mode | Register a pre-established platform-managed source populated by reproducible fixture, no file importer or arbitrary connection. | Connect to a user-supplied external spatial database now; expands credentials, connection policy, access and schema discovery scope. |

Recommendations are not approvals. Record owner answers in spec and clarify checklist first. If Q1/Q2/Q3 change scope/dependencies, revise roadmap row before treating spec as ready.

## Technical investigation (sources, not final decisions)

- PostGIS [ST_Transform](https://postgis.net/docs/ST_Transform.html) and [set-versus-transform distinction](https://postgis.net/documentation/tips/st-set-or-transform/): setting an identifier is not a coordinate transformation. Model explicit CRS declaration separately from an actual transformation and avoid silently assigning an identifier to unknown coordinates.
- PROJ [coordinate operation computation](https://proj.org/en/stable/operations/operations_computation.html) and [axis-order guidance](https://proj.org/en/stable/development/quickstart.html): coordinate operation applicability and axis order matter. Define an input/output coordinate convention, not just CRS labels; a generic geographic offset is not sufficient evidence of a precise transformation. See [glossary](https://proj.org/en/stable/glossary.html) for ballpark accuracy concerns.
- PROJ [CRS Explorer](https://crs-explorer.proj.org/) is a useful initial definition lookup. For VN-2000, verify actual authority identifiers, local central meridian, projection/scales and region against source documents, then select independently sourced control coordinates. Do not infer VN-2000 parameters from the current user's location, map appearance, or the name of a province alone.
- Specific PostGIS/PROJ versions and functions must be checked against the lockfile and actual runtime at plan time in accordance with CLAUDE.md. This is preliminary research; no library choice or runtime implementation is authorized.

## Draft data/CRS assessment to settle during clarification or planning

### Reference dataset

Propose three synthetic vector datasets (point, line, polygon families) with a total 10,000 stable feature identities, reproducible typed attribute values and representative nulls. Include a separate empty fixture and deliberate invalid/unsupported fixture. Synthetic geometry is suitable for pagination and metadata checks, but it is **not** independent geodetic ground truth. Include an independently sourced VN-2000 control-point file with provenance, permission to use, original and target coordinates, CRS/operation, units, applicability and tolerance. Choose whether Multi* and extra dimensions are in the finite matrix; explicitly report unsupported cases. Do not silently simplify or coerce.

### CRS profile controls

For every supported transformation record:
- authority identifier or exact canonical definition, axis order and coordinate units;
- datum and projection parameters, area of use, defined operation and any needed geodetic resources;
- control pairs from an independent credible source, expected tolerance and acceptance evidence;
- behavior for no matching operation/out-of-area, missing required transformation assets and incorrect metadata.
- distinguish source declaration from physical storage and each response CRS. Preserve original source facts and do not select a storage CRS by roadmap alone.

Storage strategy candidates for later ADR review if costly to reverse:
1. Preserve each source's approved native CRS in authoritative spatial storage, transforming reads to explicit response CRS; preserves local source precision, complicates multi-source operations.
2. Canonical storage CRS plus original source/definition preservation; simpler some cross-source operations, but introduces reprojection and precision/area-of-use decisions.
No winner is asserted here; planner must evaluate alongside R005 round-trip and R006 precision editing.

### Read and capability contract questions

- Return stable source/Layer/feature identifiers; whether feature geometry is encoded in a particular interchange format is a plan-level decision with a documented CRS axis convention.
- Separate technical capability, lifecycle/availability, and actor permission. R004 can depend on read contract; do not advertise render or edit as shipped merely because a source is technically compatible.
- Stable ordering, bounded page size, stale-revision behavior, null field semantics, explicit no-extent state, and safe non-disclosing errors.
- Exact permission codes and roles must be reconciled with final R002 registry. Do not hardcode names or extend roles independently on R003 branch.

## Exit gates

1. User answers Q1–Q3.
2. Review draft spec with final R002 contracts; record differences from base 98a65eb. Confirm tenancy, lifecycle, permissions and identity references.
3. Select control points, CRS matrix, tolerance, reference fixtures and local benchmark environment.
4. Resolve storage CRS/representation and revision granularity in plan/ADR if the choice is costly to reverse.
5. Only after R002 convergence and recorded prerequisite acceptance may R003 implementation start. Do not merge this isolated branch over active R002; rebase/cherry-pick documentation by an integrating owner when appropriate.
