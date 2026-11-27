//! Core domain types, error contracts, and explicit CRS registry for OpenVietGIS.

pub mod crs;
pub mod domain;
pub mod error;

pub use crs::{
    all_crs_profiles, lookup_crs, lookup_readable_crs, CoordinateBounds, CrsAxis, CrsDisposition,
    CrsPolicy, CrsProfile, CrsUnit, ProjectionKind, CRS_REGISTRY_VERSION,
    WEB_MERCATOR_MAX_COORDINATE_METRE, WEB_MERCATOR_MAX_LATITUDE,
};
pub use domain::{
    ActorContext, ActorType, Feature, FeatureId, Layer, LayerCapabilities, LayerKind, Organization,
    OrganizationStatus, Permission, Project, ProjectStatus, Revision, RevisionId, TenantScope,
    Workspace, WorkspaceStatus,
};
pub use error::{AppError, ErrorEnvelope, HttpStatusCode};
