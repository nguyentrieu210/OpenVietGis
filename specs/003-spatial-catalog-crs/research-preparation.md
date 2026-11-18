# R003 Research and Grill Preparation

**Status**: Scope clarification accepted 2026-11-18; detailed research still pending. Not a plan, implementation contract or accepted ADR.

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
| R002 implementation | Not converged at base | Block R003 plan, implementation, schema migrations and final contract until reconciliation |

One safe integration direction: R003 evaluates its own resource authorization at Project scope while reusing R002 actor/tenant context, rather than inventing a parallel authority system. No permission registry changes are accepted here.

## Three scope decisions accepted by owner

| ID | Question | A — accepted first slice | B — deferred alternative |
| --- | --- | --- | --- |
| Q1 | Ownership/reuse model | Project-scoped source and Layer; multiple Layers can reference the same source within Project. Organization always explicit. | Organization-level source catalog with cross-Project reuse; introduces independent source grants, discovery and lifecycle dependency risk. |
| Q2 | CRS support | Explicit finite matrix: WGS84, Web Mercator and chosen VN-2000 profile(s) verified by actual independent controls; reject unknown/unvalidated cases. | User-defined CRS definitions and transformation configuration, increasing correctness and operational risks. |
| Q3 | Reference source mode | Register a pre-established platform-managed source populated by reproducible fixture, no file importer or arbitrary connection. | Connect to a user-supplied external spatial database now; expands credentials, connection policy, access and schema discovery scope. |

Owner confirmed A for Q1, Q2 and Q3 in the 2026-11-18 Grill session. These are binding scope boundaries for this draft, recorded in [spec.md](spec.md#clarifications). The specific supported VN-2000 definitions, coordinate operations, control points and tolerances are not selected by that approval. Independent source evidence and final R002 contracts remain required before planning.

## Technical investigation (sources, not final decisions)

- PostGIS [ST_Transform](https://postgis.net/docs/ST_Transform.html) and [set-versus-transform distinction](https://postgis.net/documentation/tips/st-set-or-transform/): setting an identifier is not a coordinate transformation. Model explicit CRS declaration separately from an actual transformation and avoid silently assigning an identifier to unknown coordinates.
- PROJ [coordinate operation computation](https://proj.org/en/stable/operations/operations_computation.html) and [axis-order guidance](https://proj.org/en/stable/development/quickstart.html): coordinate operation applicability and axis order matter. Define an input/output coordinate convention, not just CRS labels; a generic geographic offset is not sufficient evidence of a precise transformation. See [glossary](https://proj.org/en/stable/glossary.html) for ballpark accuracy concerns.
- PROJ [CRS Explorer](https://crs-explorer.proj.org/) is a useful initial definition lookup. For VN-2000, verify actual authority identifiers, local central meridian, projection/scales and region against source documents, then select independently sourced control coordinates. Do not infer VN-2000 parameters from the current user's location, map appearance, or the name of a province alone.
- Specific PostGIS/PROJ versions and functions must be checked against the lockfile and actual runtime at plan time in accordance with CLAUDE.md. This is preliminary research; no library choice or runtime implementation is authorized.
- Official source/provision path: Vietnam's government portal publishes [Decision 83/2000/QĐ-TTg](https://vanban.chinhphu.vn/?docid=7812&pageid=27160), and the national legal database publishes [Circular 973/2001/TT-TCĐC](https://vbpl.moj.gov.vn/botainguyen/Pages/vbpq-toanvan.aspx?ItemID=138922&Keyword=973%2F2001%2FTT-TC%C4%90C). [Decision 05/2007/QĐ-BTNMT](https://vupc.mae.gov.vn/vien-tham/3491/quyet-dinh-so-05-2007-qd-btnmt-ngay-27-02-2007-ve-su-dung-he-thong-tham-so-tinh-chuyen-giua-he-toa-d) governs WGS84/VN-2000 conversion parameters. The [Ministry of Construction Circular 50/2025/TT-BXD appendix](https://moc.gov.vn/Images/FileVanBan/BXD_50-2025-TT-BXD_15122025.pdf) has now been verified to contain paired geographic VN-2000/WGS84 positions in Quang Ninh waters rounded to 0.1 arcsecond; five pairs were independently compared with the current runtime and are recorded in `vn2000-control-evidence.md`. The Ministry of Agriculture and Environment public consultation EIA for the Quang Ninh temporary transshipment project additionally publishes four points as VN-2000 TM-3 107°45' projected X/Y, VN-2000 geographic and WGS-84 geographic coordinates for the same identities. Forward and inverse EPSG:5899 checks against all four points stay below 0.2 m; a conservative 1 m acceptance tolerance is recorded for this software fixture. These public coordinates are still not a claim of cadastral/survey accuracy. Use the [government geospatial data request procedure](https://vpcp.dichvucong.gov.vn/p/home/dvc-chi-tiet-thu-tuc-hanh-chinh.html?ma_thu_tuc=1.011672) or an authorized survey/authority source before validating EPSG:3405 or EPSG:3406 transformation edges. Definition/operation discovery using [PROJ projinfo](https://proj.org/en/stable/apps/projinfo.html) is useful but is not independent control-point evidence. For WGS84/Web Mercator, prepare reproducible analytic expected-value fixtures and explicit axis/domain boundary cases.

## Research gate progress — 2026-11-18

A concrete VN-2000 evidence pass is recorded in [vn2000-control-evidence.md](vn2000-control-evidence.md). The finite candidate matrix is captured in [crs-support-matrix.md](crs-support-matrix.md), and the reproducible 10,000-feature acceptance/benchmark source is specified in [reference-fixture-profile.md](reference-fixture-profile.md).

Verified now:
- legal basis and provincial central-meridian guidance are traceable to Decision 83/2000/QD-TTg, Circular 973/2001/TT-TCDC and Decision 05/2007/QD-BTNMT;
- the current isolated runtime is PostgreSQL 18 + PostGIS 3.7.0 + PROJ 9.9.0 with network grids disabled;
- EPSG:4756, 3405, 3406 and 5899 are present in the runtime EPSG catalog, but product identity must use authority/code plus explicit metadata rather than trusting local legacy WKT names;
- five independent Quang Ninh geographic VN-2000/WGS-84 pairs from Circular 50/2025/TT-BXD were checked against local EPSG:4756 -> EPSG:4326 transformation with 1.213–1.556 m geodesic error;
- four independent Quang Ninh TM-3 107°45' projected/VN-2000 geographic/WGS-84 triples from the MAE EIA were checked in both directions using EPSG:5899, with residuals below 0.2 m;
- conservative software-fixture tolerances are 3 m for the 0.1-arcsecond geographic table and 1 m for the higher-precision EPSG:5899 table. Neither is a cadastral/survey accuracy claim.

Provisional transformation matrix permitted for **research/plan discussion only**:
- WGS84 EPSG:4326 <-> Web Mercator EPSG:3857: standard web reference conversion, with explicit axis/domain tests still required;
- VN-2000 geographic EPSG:4756 -> WGS84 EPSG:4326: independently checked in the Quang Ninh control region using the evidence above;
- selected verified VN-2000 first-slice profiles: EPSG:4756 geographic and EPSG:5899 TM-3 107-45 for the independently checked Quang Ninh control region; EPSG:5899 <-> EPSG:4756 and EPSG:5899 <-> EPSG:4326 use the 1 m fixture tolerance;
- EPSG:3405 and EPSG:3406 remain definition candidates only for R003 research; coordinate-changing transforms stay **unvalidated** until comparable independent projected controls are accepted.

This resolves the earlier uncertainty about whether any independent VN-2000 evidence was available and closes the first-slice projected control gap for EPSG:5899. The remaining hard-to-reverse technical recommendations are synthesized in [plan-preparation.md](plan-preparation.md). None of these artifacts authorize R003 planning before R002 is converged and promoted.

## R002 contract reconciliation against authoritative candidate 9010a79

This is a read-only reconciliation against the accepted R002 takeover candidate; it does not treat R002 as converged or promoted.

R002 exposes the exact project-scoped permissions `project.read`, `project.edit` and `project.manage`, plus separate Project lifecycle permissions. Human and service actors resolve through the same authorization context; service credentials can only narrow effective permissions through their ceiling. Active Organization resolution is mandatory for both actor types and inaccessible tenant resources fail as not found.

Proposed R003 action mapping, to be confirmed at plan time without adding new permission codes:

| R003 action | Existing R002 permission |
| --- | --- |
| list/inspect DataSource or Layer; schema, extent, capability and bounded feature/detail reads | `project.read` |
| register a supported source; create/rename Layer; correct attributable source/CRS catalog metadata | `project.edit` |
| retire a Layer | `project.edit` |
| retire a DataSource or perform a source-lifecycle action that can invalidate Layer references | `project.manage` |

Rationale:
- Viewer already receives `project.read`.
- Editor and Contributor receive `project.edit` as well as read, which fits ordinary GIS content/catalog work.
- Manager additionally receives `project.manage`, suitable for source-level lifecycle operations with wider dependency impact.
- R003 must call the existing permission service with explicit Organization + Project context; it must not check role names.
- Service principals must traverse the same permission path and credential ceiling as R002.
- Organization `suspended`/`deleted` state blocks ordinary access. Project `archived`/`deleted` lifecycle must be rechecked for every R003 mutation/read policy; R003 does not invent a competing lifecycle state machine.

The final plan must test direct-ID substitution and list filtering at DataSource, Layer, schema, extent, capability and feature-read boundaries using the R002 not-found/non-disclosure semantics.

## Deferred data/CRS assessment to settle at research gate

### Reference dataset

Propose three synthetic vector datasets (point, line, polygon families) with a total 10,000 stable feature identities, reproducible typed attribute values and representative nulls. Include a separate empty fixture and deliberate invalid/unsupported fixture. Synthetic geometry is suitable for pagination and metadata checks, but it is **not** independent geodetic ground truth. Include an independently sourced VN-2000 control-point file with provenance, permission to use, original and target coordinates, CRS/operation, units, applicability and tolerance. Choose whether Multi* and extra dimensions are in the finite matrix; explicitly report unsupported cases. Do not silently simplify or coerce.

### CRS profile controls

For every supported transformation record:
- authority identifier or exact canonical definition, axis order and coordinate units;
- datum and projection parameters, area of use, defined operation and any needed geodetic resources;
- control pairs from an independent credible source, expected tolerance and acceptance evidence;
- behavior for no matching operation/out-of-area, missing required transformation assets and demonstrably incorrect metadata. Explicitly declare axis ordering; reject known inconsistent fixtures or inputs, without claiming to identify mislabeled coordinates merely from plausible numeric values.
- distinguish source declaration from physical storage and each response CRS. Preserve original source facts and do not select a storage CRS by roadmap alone.

Storage strategy candidates for later ADR review if costly to reverse:
1. Preserve each source's approved native CRS in authoritative spatial storage, transforming reads to explicit response CRS; preserves local source precision, complicates multi-source operations.
2. Canonical storage CRS plus original source/definition preservation; simpler some cross-source operations, but introduces reprojection and precision/area-of-use decisions.
No winner is asserted here; planner must evaluate alongside R005 round-trip and R006 precision editing.

### Read and capability contract questions

- Return stable source/Layer/feature identifiers and stable source-scoped field identifiers distinct from display names/positions, with explicit schema revision. Whether feature geometry or attributes use a particular interchange representation is a plan-level decision with a documented CRS axis convention and typed-field mapping.
- Keep source catalog metadata/lifecycle and Layer metadata/lifecycle revisions distinct from source content revision and source schema revision; the content pin covers observed feature values, geometry/coordinate interpretation and set. Test schema/content pin changes and stale responses independently of metadata rename/lifecycle. Reserve shared Map configuration revision to R004. Physical representations/atomic versioning mechanism remain plan decisions.
- Separate named technical capability, lifecycle/availability, and current actor permission. Reconcile a stable action→accepted permission mapping for registration, inspection, metadata update and retire actions with final R002; review whether project.edit or project.manage is appropriate for retirement under the governing lifecycle, without independently defining role bundles. R004 can depend on read contract; do not advertise render or edit as shipped merely because a source is technically compatible.
- Stable ordering, bounded page size, stale content/schema revision behavior, null field semantics, explicit no-extent state, and safe non-disclosing errors. Provide a single authorized stable-ID detail read (not just scanning pages), matching the same pinned revisions and logical Layer/source/feature context. Verify the contract via an independent test consumer/harness using only R003 and its accepted prerequisites; do not require R004 runtime for R003 convergence. R004 extends the accepted contract with viewport-aware bounded rendering queries as a distinct responsibility; advanced/general query remains R007.
- Exact permission codes and roles must be reconciled with final R002 registry. Do not hardcode names or extend roles independently on R003 branch.
- Catalog history must distinguish actor/action audit from actual changed-value evidence. Preserve safe old/new values, a metadata diff or an equivalent retrievable change representation with prior revision; exclude secrets and enforce resource scope.

## Exit gates

1. **Complete** — owner accepted A for Q1–Q3 (recorded in spec).
2. **Blocked only by R002 acceptance** — review draft spec with final, converged R002 contracts; record differences from base 98a65eb. Confirm tenancy, lifecycle, permissions and identity references.
3. **Complete for first slice** — select independently verified VN-2000 definitions, transformations, control points, complete CRS matrix, tolerances, reference fixtures and local benchmark environment.
4. **Prepared for plan/ADR** — resolve storage CRS/representation, physical revision representation and atomic versioning mechanism if costly to reverse. Preserve the required semantic revision boundaries and explicit read pins; only proceed after the preceding prerequisites are accepted.
5. Only after R002 convergence, contract reconciliation and research prerequisite acceptance may R003 planning start; implementation requires a subsequent approved plan/tasks workflow. The historical 003 branch shares an older base with R002 and contains shared ROADMAP changes. Do not merge it wholesale: after R002 is accepted and promoted, the integrating owner must transplant only the feature's `specs/003-*` documents onto current verified main and reconcile ROADMAP traceability separately.
