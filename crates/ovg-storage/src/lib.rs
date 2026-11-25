//! PostgreSQL/PostGIS pool and fail-closed tenant RLS transaction primitives.

pub mod catalog;
pub mod identity;
pub mod pool;
pub mod rls;
pub mod spatial_transform;
pub mod tenancy;

pub use pool::{DbHealth, DbPool, DbPoolConfig};
pub use rls::{validate_extension_key, TenantGucBindings, TenantTransaction};
