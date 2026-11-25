# E08 Inventory & Contract Evidence — Processing, models, jobs

- **Task**: `E08-T01` (Acceptance Criterion: `E08-AC1`)
- **Parent Spec**: Spec 064 (`064-rust-platform`), references: R008
- **Child Spec**: `specs/064-rust-platform/engines/e08-processing-jobs.md`
- **Contract Artifact**: `specs/064-rust-platform/contracts/engines/e08.json`
- **Baseline Commit**: `3e78ac142acfc050ec22d30da5c44ad0e6b8459a`
- **Generated (UTC)**: `2026-11-25T17:00:35Z`
- **Architecture Candidate**: Rust/Rayon, GEOS/PostGIS/GDAL bounded job runner; optional SedonaDB/Arrow gated by benchmark.

## 1. Summary Counts

| Metric | Count |
|---|---:|
| Live HTTP Operations | 14 |
| Live WebSocket Routes | 0 |
| Live SSE / Streaming Routes | 0 / 0 |
| Classification: `migrate` | 13 |
| Classification: `replace` | 1 |
| Classification: `retain` | 0 |
| Classification: `approved_retire` | 0 |
| Owned Database Migrations | 4 |
| Mapped & Triaged Source Files | 6 |
| Mapped Pytest Files | 4 |
| Mapped Pytest Test Case IDs | 10 |

## 2. Live Runtime Operation Matrix

| Method | Path | Operation ID | Handler (`module`) | Classification | Auth / Guard | Revision / Idempotency | Tests |
|---|---|---|---|---|---|---|---:|
| `POST` | `/api/v1/organizations/{organization_id}/projects/{project_id}/pro/geometry` | `geometry_preview_api_v1_organizations__organization_id__projects__project_id__pro_geometry_post` | `geometry_preview` (`professional`) | `migrate` | session_cookie_with_csrf_or_service_token + CSRF + proj_guard(scoped:allow, FOR UPDATE) | rev-pin | 4 files (10 IDs) |
| `POST` | `/api/v1/organizations/{organization_id}/projects/{project_id}/pro/schema` | `schema_change_api_v1_organizations__organization_id__projects__project_id__pro_schema_post` | `schema_change` (`professional`) | `migrate` | session_cookie_with_csrf_or_service_token + CSRF + proj_guard(scoped:deny, FOR UPDATE) | rev-pin | 4 files (10 IDs) |
| `GET` | `/api/v1/organizations/{organization_id}/projects/{project_id}/pro/tiles/{layer_id}/{z}/{x}/{y}.mvt` | `tile_api_v1_organizations__organization_id__projects__project_id__pro_tiles__layer_id___z___x___y__mvt_get` | `tile` (`professional`) | `replace` | session_cookie_or_service_token + proj_guard(scoped:allow, FOR SHARE) | rev-pin | 4 files (10 IDs) |
| `POST` | `/api/v1/organizations/{organization_id}/projects/{project_id}/pro/topology` | `topology_api_v1_organizations__organization_id__projects__project_id__pro_topology_post` | `topology` (`professional`) | `migrate` | session_cookie_with_csrf_or_service_token + CSRF + proj_guard(scoped:deny, FOR UPDATE) | rev-pin | 4 files (10 IDs) |
| `GET` | `/api/v1/organizations/{organization_id}/projects/{project_id}/processing/algorithms` | `list_algorithms_api_v1_organizations__organization_id__projects__project_id__processing_algorithms_get` | `list_algorithms` (`processing`) | `migrate` | session_cookie_or_service_token + proj_guard(scoped:deny, FOR SHARE) | — | 3 files (8 IDs) |
| `GET` | `/api/v1/organizations/{organization_id}/projects/{project_id}/processing/jobs` | `list_processing_jobs_api_v1_organizations__organization_id__projects__project_id__processing_jobs_get` | `list_processing_jobs` (`processing`) | `migrate` | session_cookie_or_service_token + proj_guard(scoped:deny, FOR SHARE) | — | 3 files (8 IDs) |
| `POST` | `/api/v1/organizations/{organization_id}/projects/{project_id}/processing/jobs` | `create_processing_job_api_v1_organizations__organization_id__projects__project_id__processing_jobs_post` | `create_processing_job` (`processing`) | `migrate` | session_cookie_with_csrf_or_service_token + CSRF + proj_guard(scoped:deny, FOR UPDATE) | op-id | 3 files (8 IDs) |
| `GET` | `/api/v1/organizations/{organization_id}/projects/{project_id}/processing/jobs/{job_id}` | `get_processing_job_api_v1_organizations__organization_id__projects__project_id__processing_jobs__job_id__get` | `get_processing_job` (`processing`) | `migrate` | session_cookie_or_service_token + proj_guard(scoped:deny, FOR SHARE) | — | 3 files (8 IDs) |
| `POST` | `/api/v1/organizations/{organization_id}/projects/{project_id}/processing/jobs/{job_id}/cancel` | `cancel_processing_job_api_v1_organizations__organization_id__projects__project_id__processing_jobs__job_id__cancel_post` | `cancel_processing_job` (`processing`) | `migrate` | session_cookie_with_csrf_or_service_token + CSRF + proj_guard(scoped:deny, FOR UPDATE) | — | 3 files (8 IDs) |
| `POST` | `/api/v1/organizations/{organization_id}/projects/{project_id}/processing/jobs/{job_id}/retry` | `retry_processing_job_api_v1_organizations__organization_id__projects__project_id__processing_jobs__job_id__retry_post` | `retry_processing_job` (`processing`) | `migrate` | session_cookie_with_csrf_or_service_token + CSRF + proj_guard(scoped:deny, FOR UPDATE) | — | 3 files (8 IDs) |
| `GET` | `/api/v1/organizations/{organization_id}/projects/{project_id}/processing/models` | `list_models_api_v1_organizations__organization_id__projects__project_id__processing_models_get` | `list_models` (`processing_models`) | `migrate` | session_cookie_or_service_token + proj_guard(scoped:deny, FOR SHARE) | — | 3 files (8 IDs) |
| `POST` | `/api/v1/organizations/{organization_id}/projects/{project_id}/processing/models` | `create_model_api_v1_organizations__organization_id__projects__project_id__processing_models_post` | `create_model` (`processing_models`) | `migrate` | session_cookie_with_csrf_or_service_token + CSRF + proj_guard(scoped:deny, FOR UPDATE) | — | 3 files (8 IDs) |
| `DELETE` | `/api/v1/organizations/{organization_id}/projects/{project_id}/processing/models/{model_id}` | `delete_model_api_v1_organizations__organization_id__projects__project_id__processing_models__model_id__delete` | `delete_model` (`processing_models`) | `migrate` | session_cookie_with_csrf_or_service_token + CSRF + proj_guard(scoped:deny, FOR UPDATE) | — | 3 files (8 IDs) |
| `PUT` | `/api/v1/organizations/{organization_id}/projects/{project_id}/processing/models/{model_id}` | `update_model_api_v1_organizations__organization_id__projects__project_id__processing_models__model_id__put` | `update_model` (`processing_models`) | `migrate` | session_cookie_with_csrf_or_service_token + CSRF + proj_guard(scoped:deny, FOR UPDATE) | — | 3 files (8 IDs) |

## 3. Algorithms, Registries & Domain Capabilities

```json
{
  "processing_registry": {
    "source": "apps/api/openvietgis_api/domains/processing/domain.py",
    "engine": "E08",
    "statuses": [
      "queued",
      "running",
      "succeeded",
      "failed",
      "cancelled"
    ],
    "algorithms": [
      {
        "id": "buffer",
        "name": "Buffer",
        "description": "Creates projected-distance buffers (Web Mercator for geographic inputs) and publishes a new Polygon layer.",
        "required_inputs": [
          "input_layer_id"
        ],
        "parameters_schema": {
          "distance_meters": {
            "type": "number",
            "min": 0.1,
            "max": 500000.0
          },
          "quad_segs": {
            "type": "integer",
            "min": 2,
            "max": 16,
            "default": 8
          }
        },
        "source": "apps/api/openvietgis_api/domains/processing/domain.py",
        "engine": "E08"
      },
      {
        "id": "clip",
        "name": "Clip",
        "description": "Clips input layer features to an authorized overlay polygon layer.",
        "required_inputs": [
          "input_layer_id",
          "overlay_layer_id"
        ],
        "parameters_schema": {},
        "source": "apps/api/openvietgis_api/domains/processing/domain.py",
        "engine": "E08"
      },
      {
        "id": "dissolve",
        "name": "Dissolve",
        "description": "Unions features globally or grouped by an attribute field into a new layer.",
        "required_inputs": [
          "input_layer_id"
        ],
        "parameters_schema": {
          "group_by_field": {
            "type": "string",
            "nullable": true
          }
        },
        "source": "apps/api/openvietgis_api/domains/processing/domain.py",
        "engine": "E08"
      }
    ]
  },
  "processing_model_dag_algorithms": {
    "source": "apps/api/openvietgis_api/adapters/http/processing_models.py",
    "engine": "E08",
    "cross_engine_raster_nodes": [
      "band",
      "ndvi",
      "ndwi",
      "ndbi",
      "hillshade",
      "slope",
      "aspect",
      "resample",
      "mosaic",
      "change",
      "rgb",
      "calculator"
    ],
    "algorithms": [
      "buffer",
      "clip",
      "dissolve",
      "band",
      "ndvi",
      "ndwi",
      "ndbi",
      "hillshade",
      "slope",
      "aspect",
      "resample",
      "mosaic",
      "change",
      "rgb",
      "calculator"
    ]
  },
  "professional_operations": {
    "source": "apps/api/openvietgis_api/adapters/http/professional.py",
    "engine": "E08",
    "geometry_operations": [
      "move_batch",
      "copy",
      "offset",
      "rotate",
      "scale",
      "stretch",
      "extend",
      "trim",
      "fillet",
      "intersection",
      "union",
      "difference",
      "centroid",
      "hull",
      "simplify",
      "nearest",
      "spatial_join"
    ],
    "topology_checks": [
      "duplicate",
      "invalid",
      "overlap",
      "dangling",
      "outside",
      "gap"
    ],
    "schema_actions": [
      "add",
      "rename"
    ],
    "schema_field_types": [
      "text",
      "decimal",
      "integer",
      "boolean",
      "timestamp"
    ]
  }
}
```

## 4. Owned Database Migrations

| Revision | Down Revision | File | RLS | Classification | Summary |
|---|---|---|---|---|---|
| `0011_processing_jobs` | `0010_saved_queries` | `apps/api/migrations/versions/0011_processing_jobs.py` | yes | `retain` | R008 Spatial Processing & Durable Jobs schema with RLS and derived source lineage. |
| `0021_professional_tools` | `0020_manual_layers` | `apps/api/migrations/versions/0021_professional_tools.py` | no | `retain` | Spatial lookup index and ISO A0–A4 cartography. |
| `0022_schema_change_evidence` | `0021_professional_tools` | `apps/api/migrations/versions/0022_schema_change_evidence.py` | no | `retain` | A separate catalog operation for revision-pinned schema changes. |
| `0025_processing_models` | `0024_raster_layers` | `apps/api/migrations/versions/0025_processing_models.py` | yes | `retain` | Project-scoped reusable GIS processing models. |

## 5. Mapped & Triaged Source Files

Total source files mapped to `E08`: **6**

- `apps/api/openvietgis_api/adapters/http/processing.py` (`migrate` — static-inventory direct mapping)
- `apps/api/openvietgis_api/adapters/http/processing_models.py` (`migrate` — static-inventory direct mapping)
- `apps/api/openvietgis_api/adapters/http/professional.py` (`migrate` — static-inventory direct mapping)
- `apps/api/openvietgis_api/adapters/persistence/processing_repository.py` (`migrate` — static-inventory direct mapping)
- `apps/api/openvietgis_api/domains/processing/__init__.py` (`migrate` — static-inventory direct mapping)
- `apps/api/openvietgis_api/domains/processing/domain.py` (`migrate` — static-inventory direct mapping)

## 6. Cross-Engine Dependencies

- **E01**: All tenant/project routes depend on E01 authentication, CSRF, RBAC/ACL, RLS, and project_access_guard.
- **E02**: Reads authorized E02 input layers and publishes derived output sources/layers.
- **E05**: Hosts /pro/tiles/{layer_id}/{z}/{x}/{y}.mvt which converges with E05 tile gateway.
- **E09**: Processing model DAGs include 12 raster algorithm nodes executed against E09 raster primitives.

## 7. Existing Pytest Evidence & Test Identifiers

Mapped **4 pytest files** containing **10 test case IDs** (primary + cross-engine coverage):

- `apps/api/tests/integration/m2/test_processing_integrity.py` (primary, 4 tests): `test_stale_job_fails_durably_without_repinning`, `test_clip_retains_family_and_empty_output`, `test_atomic_rollback_on_publication_failure_and_consumers`, `test_idempotency_and_null_dissolve_groups`
- `apps/api/tests/integration/spatial/test_processing_models.py` (primary, 1 tests): `test_saved_processing_graph_and_real_chain`
- `apps/api/tests/integration/spatial/test_professional.py` (primary, 2 tests): `test_pro_tools`, `test_projected_native_tile_is_revision_pinned`
- `apps/api/tests/unit/test_processing_domain.py` (primary, 3 tests): `test_buffer_valid_distance`, `test_invalid_buffer_parameter_is_domain_error`, `test_unknown_parameter_rejected`

## 8. Edge Cases & Parity Risks

- Processing parameter validation must reject booleans/strings coerced as numbers, NaN/Inf, and out-of-range distance_meters (0.1..500000) or quad_segs (2..16).
- DAG cycle detection, invalid step/layer references, and arity validation (binary ops, 3-band RGB, calculator formula 'B' ref) in processing_models.
- Job cancellation, retry idempotency, bounded worker concurrency/memory, and recovery after worker crash.
- Professional geometry/topology/schema operations must honor pinned revisions, scoped ACL, and fillet interior-vertex preconditions.

## 9. E08-AC1 Verification

- [x] Live FastAPI runtime instantiated via `create_app()` from worktree (`3e78ac1`), zero mocked route tables.
- [x] All **14** live runtime operations in `E08` extracted with exact OpenAPI method, path, operationId, schemas, auth/CSRF/guard dependencies, and revision/CRS/error semantics.
- [x] 100% of `E08` operations explicitly classified (`migrate` / `retain` / `replace` / `approved_retire`); zero unclassified or silently dropped routes.
- [x] Algorithms/capabilities, **4** owned migrations, **6** source files, and **4** pytest files (**10** test IDs) linked in `contracts/engines/e08.json`.
- [ ] Next phase (`E08-T02`): expand golden/negative/permission fixtures for any operation relying only on engine-level suite coverage.
