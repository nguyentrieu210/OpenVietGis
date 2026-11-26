# E08 Rust-Native Parity Evidence

- **Branch**: `064-e08-rust-native`
- **Base**: `064-convergence`
- **Scope**: Port all 14 E08 operations (`processing.py`, `processing_models.py`, `professional.py`) to Rust-native control plane + bounded Rayon execution (`ovg-processing::JobExecutor`) + PostGIS storage (`ovg-storage`), with differential PostGIS parity against the FastAPI oracle.

## E08 Route Coverage (14 / 14)

| # | Method | Path | Rust Handler | Storage / Engine |
|---|---|---|---|---|
| 1 | `GET` | `/processing/algorithms` | `processing::list_algorithms` | `ovg_processing::registry` |
| 2 | `POST` | `/processing/jobs` | `processing::create_job` | `ovg_storage::processing_jobs` + `ovg_processing::JobExecutor` |
| 3 | `GET` | `/processing/jobs` | `processing::list_jobs` | `ovg_storage::processing_jobs` |
| 4 | `GET` | `/processing/jobs/{job_id}` | `processing::get_job` | `ovg_storage::processing_jobs` |
| 5 | `POST` | `/processing/jobs/{job_id}/cancel` | `processing::cancel_job` | `ovg_storage::processing_jobs` + `JobExecutor::cancel_job` |
| 6 | `POST` | `/processing/jobs/{job_id}/retry` | `processing::retry_job` | `ovg_storage::processing_jobs` + `ovg_processing::JobExecutor` |
| 7 | `GET` | `/processing/models` | `processing_models::list_models` | `ovg_storage::processing_models` |
| 8 | `POST` | `/processing/models` | `processing_models::create_model` | `ovg_storage::processing_models` |
| 9 | `PUT` | `/processing/models/{model_id}` | `processing_models::update_model` | `ovg_storage::processing_models` |
| 10 | `DELETE` | `/processing/models/{model_id}` | `processing_models::delete_model` | `ovg_storage::processing_models` |
| 11 | `POST` | `/pro/geometry` | `professional::geometry_preview` | `ovg_storage::professional` |
| 12 | `POST` | `/pro/topology` | `professional::topology` | `ovg_storage::professional` |
| 13 | `POST` | `/pro/schema` | `professional::schema_change` | `ovg_storage::professional` + `ovg_storage::spatial_pins` |
| 14 | `GET` | `/pro/tiles/{layer_id}/{z}/{x}/{y}.mvt` | `professional::tile` | `ovg_storage::professional` |

## Verification Status

Pending GitHub Actions CI run on PR into `064-convergence`.
