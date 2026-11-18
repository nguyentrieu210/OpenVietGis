# R003 Plan Preparation

**Status**: pre-plan technical synthesis only. This file is intentionally **not** `plan.md`; R003 planning and implementation remain blocked until R002 convergence and verified-main promotion.

**Prepared**: 2026-11-18

## Inputs already reconciled

- R003 specification and requirements-quality checklist.
- R002 authoritative candidate `9010a79`: explicit Organization/Project context, human + service actor semantics, permission ceiling, active tenant rules and current Project lifecycle.
- ADR-005 PostgreSQL/PostGIS source of truth.
- ADR-006 Layer/capability abstraction.
- ADR-007 revision/optimistic-concurrency semantics.
- ADR-010 source artifact boundary.
- ADR-014 explicit tenant context + RLS defense-in-depth.
- `crs-support-matrix.md`, `vn2000-control-evidence.md`, and `reference-fixture-profile.md`.

## Recommended irreversible decisions for the later plan

### 1. Preserve authoritative feature coordinates in the source's approved native CRS

**Recommendation**: choose native-CRS authoritative storage for the first source adapter.

Reasoning:
- R003 explicitly forbids silently rewriting source coordinates.
- R005 will need round-trip/provenance semantics.
- R006 precision editing should not inherit an avoidable up-front reprojection.
- Read-time response transformation is already explicit and bounded by the accepted CRS matrix.

Consequences:
- every source records `source_crs` and `stored_crs`; they may be equal in the first adapter;
- response CRS is a request/response concern and never mutates source rows;
- future multi-source operations must normalize explicitly rather than assume one storage CRS.

Do not canonicalize all authoritative geometry to EPSG:4326 or 3857 during R003 registration.

### 2. Keep physical source location private behind a DataSource adapter

The public DataSource identity must not be a table name/path.

Recommended internal split:
- public DataSource row: ownership, logical identity, provenance, state, CRS/schema/extent summaries and revision identities;
- private storage binding: adapter kind + opaque internal locator;
- reference adapter: only allow an allowlisted platform-managed fixture key;
- no user-supplied SQL, schema/table path, external URL, password or arbitrary database DSN in R003.

This lets R005/R010 introduce new adapters without changing Layer identity.

### 3. Separate four revision domains physically and semantically

Recommended revision fields:

- DataSource `catalog_revision`: monotonic integer for metadata/lifecycle changes;
- Layer `layer_revision`: monotonic integer for Layer metadata/lifecycle changes;
- DataSource `content_revision`: opaque immutable token identifying the observed feature set/values/geometry interpretation;
- DataSource `schema_revision`: opaque immutable token identifying logical field identities/types/nullability + geometry schema.

For the first deterministic reference adapter, content/schema tokens can be deterministic UUID/hash values generated from fixture manifests. They must not be derived from mutable timestamps alone.

Rules:
- metadata-only rename increments catalog/layer revision but not content/schema revision;
- content-only fixture change replaces content token only;
- schema change replaces schema token and also content token when field interpretation or geometry interpretation changes;
- page/detail requests send the expected content + schema tokens;
- mismatch returns an explicit stale/retry response rather than serving a mixed observation.

Historical feature snapshots are not required in R003. A stale pinned revision may fail if no longer current; it must never be silently substituted.

### 4. Use optimistic concurrency on mutable catalog resources

Recommended mutation contract:
- client sends `expected_revision` for DataSource or Layer metadata/lifecycle mutation;
- repository update predicates on current revision and increments atomically;
- zero updated rows becomes a deterministic conflict;
- successful mutation + safe change evidence + audit event occur in one transaction.

Do not hold a transaction open across a user editing session.

### 5. Use an explicit X/Y geometry contract, not misleading arbitrary-CRS GeoJSON

GeoJSON semantics strongly imply WGS84 longitude/latitude and no longer define arbitrary CRS members. R003 needs explicit projected VN-2000 reads.

Recommended public geometry envelope:

```json
{
  "type": "Point",
  "coordinates": [438442.69, 2291039.83],
  "coordinate_reference": {
    "authority": "EPSG",
    "code": 5899,
    "coordinate_order": "xy",
    "x_axis": "easting",
    "y_axis": "northing",
    "unit": "metre"
  }
}
```

For geographic responses, X = longitude and Y = latitude even though EPSG:4326/EPSG:4756 native metadata lists latitude then longitude. Catalog metadata must separately expose the EPSG-native axes so no one confuses authority metadata with the application interchange convention.

The final plan must name this object something other than "GeoJSON" unless it is constrained to RFC 7946 semantics.

### 6. Prefer stable-key cursor paging over offset paging

Recommended request contract:
- `limit` 1..100;
- explicit `content_revision`;
- explicit `schema_revision`;
- optional `after_feature_id` stable cursor key;
- deterministic ascending feature identity ordering.

Benefits:
- no dependence on heap/insertion order;
- easier exact-once traversal proof;
- stale revisions fail before page continuation;
- no server-side cursor/session state.

If feature identities are not lexically sortable in a future adapter, expose an opaque signed cursor containing only stable ordering information + revision pins. Do not expose physical row IDs as platform feature identity.

### 7. Make retirement non-destructive and dependency-safe

Recommended first-slice state:
- DataSource: `active`, `invalid`/not-ready as validation state, `retired` lifecycle;
- Layer: `active`, `retired`.

Rules:
- retiring a Layer never deletes feature data or source;
- retiring a DataSource with any active Layer is rejected with dependency guidance;
- no physical source purge in R003;
- retired resources remain addressable only where the accepted historical/audit contract permits, never as ordinary readable active catalog data;
- restoration can be deferred unless the spec explicitly accepts it before planning.

This resolves FR-011 using the safest first-slice behavior: reject source retirement while active references exist.

### 8. Reuse R002 permissions exactly

| Action | Permission |
| --- | --- |
| list/detail/schema/extent/capability/page/feature detail | `project.read` |
| register source, create Layer, rename/update catalog metadata, retire Layer | `project.edit` |
| retire source with dependency impact | `project.manage` |

Every path:
1. authenticates human or service credential;
2. receives explicit Organization and Project;
3. validates active Organization + accessible Project;
4. resolves existing R002 effective permission/credential ceiling;
5. sets transaction-local RLS context;
6. performs repository work.

No R003 role checks, sticky active tenant or independent permission registry.

### 9. Put RLS on tenant-owned catalog/change rows

Candidate protected tables:
- spatial.data_sources;
- spatial.layers;
- spatial.source_fields;
- spatial.catalog_changes;
- adapter-visible reference feature rows if stored in a tenant-owned generic table.

Each tenant-owned row carries `organization_id`; Project-scoped rows also carry `project_id`. Database constraints must prevent a Layer from referencing a DataSource with a different Project/Organization even if application code is wrong.

The runtime role remains non-owner/non-BYPASSRLS and tests must use it.

### 10. Store safe change evidence separately from broad security audit semantics

R002 audit proves actor/action/security history. R003 also requires actual changed-value evidence for catalog revisions.

Recommended catalog change row:
- organization_id, project_id;
- resource type/id;
- actor type/id + display snapshot or compatible audit reference;
- operation;
- prior_revision / new_revision;
- safe structured diff for changed catalog fields;
- timestamp.

Never store source credentials, physical private locators or raw large feature payloads in the diff.

A mutation transaction writes the resource update and required change evidence atomically. The existing audit append remains part of the same logical mutation where the R002 audit contract requires it.

## Candidate API surface for plan review

Names are preparatory, not accepted OpenAPI yet.

```text
GET    /organizations/{org}/projects/{project}/spatial/sources
POST   /organizations/{org}/projects/{project}/spatial/sources
GET    /organizations/{org}/projects/{project}/spatial/sources/{source}
PATCH  /organizations/{org}/projects/{project}/spatial/sources/{source}
POST   /organizations/{org}/projects/{project}/spatial/sources/{source}/retire

GET    /organizations/{org}/projects/{project}/layers
POST   /organizations/{org}/projects/{project}/layers
GET    /organizations/{org}/projects/{project}/layers/{layer}
PATCH  /organizations/{org}/projects/{project}/layers/{layer}
POST   /organizations/{org}/projects/{project}/layers/{layer}/retire

GET    /organizations/{org}/projects/{project}/layers/{layer}/features
GET    /organizations/{org}/projects/{project}/layers/{layer}/features/{feature_id}
```

Schema, extent and capabilities can be part of Layer/Source detail when small and stable, or dedicated read endpoints if contract size/caching requires it. The plan should avoid endpoint proliferation without a concrete need.

All protected IDs remain scoped by explicit org + project in the route even when globally unique UUIDs are used.

## Candidate persistence shape for plan review

No migration is authorized yet. Expected logical tables:

- `spatial.data_sources`
- `spatial.source_fields`
- `spatial.layers`
- `spatial.catalog_changes`
- platform-managed reference feature storage/fixture tables hidden behind the adapter

Important database invariants:
- unique source/Layer ID plus explicit organization/project ownership;
- Layer -> DataSource composite ownership constraint;
- stable field identity unique within source/schema semantics;
- revision > 0;
- lifecycle/validation state constraints;
- FORCE RLS where tenant-sensitive;
- no foreign key that permits cross-Project sharing in R003.

## Service boundaries

Recommended domain boundaries:

```text
domains/spatial_catalog/
  domain.py
  ports.py
  service.py
  crs.py
  revisions.py

adapters/persistence/
  spatial_catalog_repository.py
  reference_source_adapter.py

adapters/http/
  spatial_catalog.py
```

The CRS registry is domain/application policy around a finite allowlist + evidence, while coordinate transformation execution is an adapter to PostGIS/PROJ. Core domain code must not depend on a renderer.

## Required plan-time negative tests

The final plan/tasks must explicitly include:
- cross-Organization and cross-Project source/Layer direct-ID substitution;
- same actor authorized in both Projects still cannot cross-reference source;
- service credential ceiling denies a technically possible read/mutation;
- revoked access fails on the next request;
- suspended Organization, archived/deleted Project lifecycle behavior;
- invalid/missing/unsupported CRS;
- axis-swapped known-control failure;
- out-of-area EPSG:5899 transform request;
- Z/M and mixed geometry rejection;
- empty source no-extent semantics;
- stale catalog/layer mutation revision;
- stale content/schema page/detail revision;
- source retirement blocked by active Layer;
- Layer retirement leaves source + sibling Layer untouched;
- interrupted mutation leaves no ready partial resource or success change record;
- logs/audit/output expose no private storage locator or secret.

## Open items deliberately left for the real plan

These are execution choices, not unresolved product requirements:
- exact Python class/module names after rebasing on promoted main;
- exact SQL DDL/index names;
- whether small schema/extent summaries are embedded or dedicated endpoints;
- cursor serialization format if opaque cursors are chosen;
- exact fixture generation implementation;
- OpenAPI component names.

They should be decided only after the R002 accepted source is the actual planning baseline.
