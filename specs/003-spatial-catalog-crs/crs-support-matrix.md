# R003 CRS Support Matrix

**Status**: research gate candidate; runtime implementation remains blocked until R002 convergence/promotion and R003 plan/tasks approval.

**Captured**: 2026-11-18

## Core rule

A CRS definition being known does not make every transformation involving it supported. R003 must model separately:

1. CRS identity/metadata;
2. transformation edge and operation identity;
3. area of applicability;
4. independent control evidence;
5. accepted tolerance;
6. API coordinate convention.

Unknown or unvalidated edges fail explicitly.

## First-slice CRS definitions

| CRS | Name | EPSG-native axes | Unit | R003 disposition |
| --- | --- | --- | --- | --- |
| EPSG:4326 | WGS 84 | latitude, longitude | degree | supported definition |
| EPSG:3857 | WGS 84 / Pseudo-Mercator | easting, northing (X,Y) | metre | supported definition for web/reference reads |
| EPSG:4756 | VN-2000 | latitude, longitude | degree | supported VN-2000 geographic definition |
| EPSG:5899 | VN-2000 / TM-3 107-45 | easting, northing (E,N) | metre | selected verified projected VN-2000 profile for the Quang Ninh first slice |
| EPSG:3405 | VN-2000 / UTM zone 48N | easting, northing | metre | known definition only; transformation edges not accepted in R003 first slice |
| EPSG:3406 | VN-2000 / UTM zone 49N | easting, northing | metre | known definition only; transformation edges not accepted in R003 first slice |

Authoritative metadata references:
- https://epsg.io/4326
- https://epsg.io/3857
- https://epsg.io/4756
- https://epsg.io/5899
- https://epsg.io/3405
- https://epsg.io/3406

EPSG:5899 covers Quang Ninh among its listed areas of use and uses metre easting/northing axes. EPSG lists VN-2000 to WGS 84 (2), operation 6960, with stated 1 m accuracy for Vietnam onshore.

## Transformation matrix

Legend:
- **VALIDATED**: may be advertised by the R003 first slice when the request lies inside the stated control/applicability rules.
- **ANALYTIC**: deterministic mathematical control fixture, not a geodetic datum-shift claim.
- **UNVALIDATED**: CRS definition may be known but the coordinate-changing edge must be rejected.

| Source | Target | Status | Evidence / operation | Acceptance |
| --- | --- | --- | --- | --- |
| 4326 | 3857 | ANALYTIC | EPSG Pseudo-Mercator definition; independent formula fixture | exact/formula tolerance defined in test, within valid latitude domain |
| 3857 | 4326 | ANALYTIC | inverse Pseudo-Mercator formula fixture | exact/formula tolerance defined in test |
| 4756 | 4326 | VALIDATED | independent Quang Ninh geographic pairs; EPSG operation 6960 | <= 3 m for 0.1-arcsecond public control table |
| 4326 | 4756 | VALIDATED | same independent source coordinates, inverse evaluated from published target rather than implementation round-trip | <= 3 m |
| 5899 | 4756 | VALIDATED | MAE Quang Ninh EIA projected + VN-2000 geographic triples | <= 1 m |
| 4756 | 5899 | VALIDATED | same independent triples, inverse from published coordinates | <= 1 m |
| 5899 | 4326 | VALIDATED | MAE Quang Ninh EIA projected + WGS-84 triples; EPSG 6960 datum shift | <= 1 m |
| 4326 | 5899 | VALIDATED | same independent triples, inverse from published WGS-84 targets | <= 1 m |
| 3405 | any different CRS | UNVALIDATED | no accepted independent projected control fixture yet | reject |
| 3406 | any different CRS | UNVALIDATED | no accepted independent projected control fixture yet | reject |
| arbitrary/custom CRS | any | UNVALIDATED/out of scope | R003 excludes user-defined definitions | reject |

Identity/no-coordinate-change reads in a known source CRS are not transformation edges. They still require explicit CRS identity and coordinate convention.

## WGS84 / Web Mercator analytic controls

The plan must implement controls independently from PostGIS/PROJ using the published Pseudo-Mercator equations, at minimum:
- (longitude 0°, latitude 0°) <-> (0 m, 0 m);
- deterministic Vietnam points in the synthetic reference fixture;
- longitude boundary near +/-180°;
- valid Web-Mercator latitude limit and deliberately out-of-domain latitude.

Do not use a transform-then-inverse round-trip as the only expected value.

## VN-2000 independent controls

Detailed measurements, legal/source provenance and forward/inverse residuals are in:
- [vn2000-control-evidence.md](vn2000-control-evidence.md)

The selected projected first-slice profile is intentionally **EPSG:5899**, not a generic string "VN-2000". Its Quang Ninh use is traceable to the 107°45' 3-degree projection and independent paired coordinates.

## Axis-order and application coordinate convention

EPSG:4326 and EPSG:4756 formally define ellipsoidal axes as **latitude, longitude**, while PostGIS geometry X/Y and common web/GeoJSON practice are normally **longitude/easting, latitude/northing**. EPSG:3857 and EPSG:5899 use projected easting/northing axes.

The R003 plan MUST choose and document one public geometry coordinate convention instead of inheriting library defaults. Recommended plan decision:

- public coordinate arrays are always explicit **X,Y**;
- for geographic CRS, X = longitude and Y = latitude;
- for projected CRS, X = easting and Y = northing;
- every coordinate-bearing response includes the CRS authority/code and the declared application convention;
- catalog CRS metadata separately reports the EPSG-native axis definitions;
- no endpoint silently swaps axes based on library behavior;
- known axis-swapped negative fixtures fail coordinate/domain validation where independently detectable.

If the planner chooses a different convention, it must change the contract and controls consistently before implementation.

## Area/applicability handling

- EPSG:3857: only its published web-mapping latitude domain is accepted.
- EPSG:4756 / operation 6960: Vietnam onshore; the first independent acceptance fixture is Quang Ninh.
- EPSG:5899: only areas covered by the EPSG definition may be considered, and R003 first-slice validated transformation evidence is specifically the Quang Ninh fixture.
- "Inside numerical bounds" is not enough to prove a mislabeled CRS. R003 rejects known contradictions but does not claim to infer a CRS from plausible coordinate values.

## Runtime pin

Research measurements were performed against:
- PostgreSQL 18;
- PostGIS 3.7.0;
- PROJ 9.9.0;
- PROJ network grids disabled.

The plan must record the deployed versions and operation selected by the runtime. A runtime/library upgrade that changes the selected transformation or its required assets must re-run control evidence before acceptance.

## Exit condition

This matrix is ready for planning once R002 is converged/promoted. The planner must convert it into:
- immutable CRS/profile metadata fixtures;
- explicit supported transformation registry;
- independent control tests;
- error contracts for unsupported/out-of-area/unvalidated operations;
- the chosen public X/Y interchange contract.
