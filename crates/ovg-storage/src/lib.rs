//! PostgreSQL/PostGIS pool and fail-closed tenant RLS transaction primitives.

pub mod cad_documents;
pub mod cartography;
pub mod catalog;
pub mod e11_outbox;
pub mod editing;
pub mod field_packages;
pub mod identity;
pub mod integration_hub;
pub mod pool;
pub mod processing_jobs;
pub mod queries;
pub mod raster;
pub mod rls;
pub mod spatial_transform;
pub mod tenancy;
pub mod transfers;
pub mod viewport;

pub use pool::{DbHealth, DbPool, DbPoolConfig};
pub use rls::{validate_extension_key, TenantGucBindings, TenantTransaction};
