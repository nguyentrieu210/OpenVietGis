# R003 Reference Fixture and Benchmark Profile

**Status**: research preparation; implementation remains gated by R002 convergence/promotion.

**Captured**: 2026-11-18

## Purpose

R003 needs one reproducible, platform-managed spatial source that exercises catalog discovery, Layer identity, bounded feature reads, CRS metadata, revision pins, tenant isolation and performance without implementing R005 file ingestion or R010 external connections.

The fixture is generated from code/SQL checked into the repository. No user-uploaded file or private connection is required.

## Supported first-slice geometry matrix

The minimum accepted readable geometry families are deliberately finite:

| Family | Dimensions | First-slice disposition |
| --- | --- | --- |
| Point | XY / 2D | supported |
| LineString | XY / 2D | supported |
| Polygon | XY / 2D | supported |
| MultiPoint | any | explicitly unsupported in the first slice |
| MultiLineString | any | explicitly unsupported in the first slice |
| MultiPolygon | any | explicitly unsupported in the first slice |
| GeometryCollection | any | explicitly unsupported |
| Any Z or M dimension | Z/M | explicitly unsupported until a later spec accepts precision/dimension semantics |

A source fixture may contain null geometry only in an explicit negative/availability fixture. A ready source cannot silently coerce mixed families or unsupported dimensions.

## Primary 10,000-feature benchmark source

Create three deterministic physical fixture tables owned by the platform test fixture, all under one synthetic Project and one synthetic Organization:

| Dataset | Features | Native CRS | Geometry |
| --- | ---: | --- | --- |
| reference_points_4326 | 4,000 | EPSG:4326 | Point XY |
| reference_lines_4326 | 3,000 | EPSG:4326 | LineString XY |
| reference_polygons_4326 | 3,000 | EPSG:4326 | Polygon XY |

Total: **10,000 features**.

All geometries stay inside an explicit Vietnam test envelope and are created by deterministic formulas from the numeric feature sequence. The fixture generator must produce identical coordinates and attributes on every clean setup.

The benchmark source is synthetic and is not geodetic truth. VN-2000 control evidence remains in `vn2000-control-evidence.md`.

## Stable logical schema

Each source exposes stable field identities that are not inferred from display order:

| Stable field ID | Display name | Type | Nullable | Deterministic value |
| --- | --- | --- | --- | --- |
| fld_feature_code | Feature code | text | no | family prefix + zero-padded sequence |
| fld_name | Name | text | no | deterministic label |
| fld_rank | Rank | integer | no | sequence modulo a fixed bound |
| fld_measure | Measure | decimal | yes | deterministic decimal; null on a documented modulus |
| fld_active | Active | boolean | no | deterministic parity |
| fld_observed_at | Observed at | timestamp with timezone | yes | fixed epoch + deterministic interval; null on a documented modulus |
| fld_note | Note | text | yes | includes null, empty string and non-empty values as distinct cases |

Physical column names are not part of the public stable identity contract. The fixture must explicitly map each logical field ID to its physical column.

## Stable feature identity and ordering

- Feature identity is source-scoped and immutable for the life of a source content revision.
- Fixture feature IDs use deterministic values such as `point-000001`, `line-000001`, `polygon-000001`.
- Default page ordering is by the stable feature identity, not insertion order or database heap order.
- The first page limit is **100 features**; requests above 100 must fail or clamp only if the contract explicitly documents clamping. Prefer explicit validation failure.
- Page traversal must carry both `content_revision` and `schema_revision`. If either no longer matches, return an explicit stale/retry result instead of mixing revisions.

## Required companion fixtures

### Empty source

- valid supported CRS and schema;
- zero features;
- count = 0;
- extent = absent/null with an explicit empty state, never a fabricated world extent.

### Unknown-CRS source

- valid feature rows whose CRS is deliberately unset/unknown;
- registration can retain truthful provenance but cannot mark coordinate-dependent reads ready;
- coordinates are never rewritten or assigned a guessed SRID.

### Contradictory metadata source

At least one deterministic source where declared geometry family or CRS conflicts with inspected data. Registration/validation must reject ready status and return an actionable, non-secret reason.

### Unsupported-dimension source

A Z geometry fixture proving R003 reports the unsupported dimension rather than dropping Z or coercing it to XY.

### Cross-project source

A valid platform-managed reference source owned by Project B, used only to prove Project A cannot create a Layer pointing at it even when the same actor is authorized in both Projects.

### Revision-change fixture

A controlled test helper produces:
1. content-only revision change with stable schema;
2. schema revision change preserving unrelated field identity;
3. metadata-only catalog rename with unchanged content/schema revisions.

These cases prove the three revision domains are not conflated.

## CRS control fixtures

The accepted first-slice CRS definitions and transformations are tracked separately in `vn2000-control-evidence.md`.

Minimum control groups:
- EPSG:4326 definition and axis/unit metadata;
- EPSG:3857 definition and analytic 4326 <-> 3857 controls, including domain edge cases;
- EPSG:4756 VN-2000 geographic definition plus independent Quang Ninh -> EPSG:4326 controls;
- EPSG:5899 VN-2000 / TM-3 107-45 plus independent Quang Ninh projected <-> 4756 and projected <-> 4326 controls.

EPSG:3405/3406 may be listed as known definitions during research but their coordinate-changing edges are not accepted until independent projected controls exist.

## Capability fixture

A registered first-slice Layer advertises only capabilities that R003 actually implements:
- catalog metadata read;
- schema read;
- extent/count state read;
- bounded feature page read;
- stable-ID feature detail read;
- supported explicit response-CRS transformation.

It must not advertise map rendering, styling, editing, export, generic spatial query, processing, offline, CAD or file import.

Technical capability, resource availability and current actor permission are separate fields/concepts.

## Benchmark environment

Acceptance benchmark profile:

- PostgreSQL 18;
- PostGIS 3.7.x;
- PROJ 9.9.x with network grids disabled unless an accepted profile explicitly changes that;
- the deterministic 10,000-feature fixture above;
- restricted `openvietgis_runtime` database role with RLS enabled;
- warmed execution before measurement;
- at least 100 measured operations for each target;
- no debug profiler in the measured request path.

Measure separately:

1. catalog list/detail read: p95 <= **1,000 ms**;
2. bounded page of 100 features: p95 <= **2,000 ms**.

Record machine/runner identifier, database/PostGIS/PROJ versions, fixture revision, sample count, warm-up count, p50/p95/max and whether response CRS transformation was requested.

The CI benchmark is authoritative for regression. A local benchmark may supplement it but must not replace the recorded environment.

## Independent contract consumer

R003 acceptance includes a small contract-test consumer that knows only:
- Organization and Project context;
- public catalog/Layer identifiers;
- schema and capability output;
- content/schema revision pins;
- bounded page and stable feature-detail contract.

It must not use physical table names, storage schema names, SQL, renderer internals or R004 code. This is the forward-compatibility proof for R004 without implementing R004.

## Exit condition

This profile is ready for use by the R003 planner once:
1. R002 is converged and promoted;
2. the exact CRS support matrix is accepted alongside the control evidence;
3. planning chooses the physical revision/versioning representation and API coordinate interchange without weakening the semantic requirements above.
