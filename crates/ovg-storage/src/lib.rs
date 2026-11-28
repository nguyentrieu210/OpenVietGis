//! PostgreSQL/PostGIS pool and fail-closed tenant RLS transaction primitives.

pub mod cad_documents;
pub mod cartography;
pub mod catalog;
pub mod collaboration;
pub mod e11_outbox;
pub mod editing;
pub mod extensions;
pub mod external_sources;
pub mod field_packages;
pub mod gis_access;
pub mod house_numbering;
pub mod identity;
pub mod integration_hub;
mod native_projection;
pub mod pool;
pub mod processing_jobs;
pub mod processing_models;
pub mod professional;
pub mod queries;
pub mod raster;
pub mod rls;
pub mod spatial_pins;
pub mod spatial_transform;
pub mod tenancy;
pub mod three_d;
pub mod tilesets;
pub mod transfers;
pub mod viewport;

pub use pool::{DbHealth, DbPool, DbPoolConfig};
pub use rls::{validate_extension_key, TenantGucBindings, TenantTransaction};
