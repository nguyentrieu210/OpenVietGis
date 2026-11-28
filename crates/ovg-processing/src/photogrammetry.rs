//! Typed, fail-closed photogrammetry provider control-plane contracts.
//! No execution, filesystem access or network capability is granted by these types.
pub mod colmap;
pub mod governor;
pub mod quality;
pub mod hardware;
pub mod tiles;
pub mod admission;
pub mod images;
pub mod octree;
pub mod dag;
pub mod scheduler;
pub mod manifest;
pub mod matching;
pub mod geometry;
pub mod survey;
pub mod ortho;
pub mod depth;
pub mod surface;
pub mod seam;
pub mod mesh;

use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

pub const PROTOCOL_MAJOR: u16 = 2;
pub const PROTOCOL_MINOR: u16 = 0;
pub const MAX_HANDLES: usize = 4096;
pub const MAX_DEPENDENCIES: usize = 4096;
pub const MAX_CONTROL_BYTES: usize = 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Operation { InspectImages, ExtractFeatures, MatchFeatures, SparseReconstruction, DenseReconstruction, Orthorectify, BuildSurface, ExportArtifact }

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Format { Jpeg, Tiff, CameraGraph, FeatureIndex, DepthMap, Surface, Cog, Copc, Glb }

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Device { Cpu, Cuda, Hip, Metal }

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Precision { F32, F64, MixedValidated }

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Access { Read, Write }

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Budget { pub host_bytes: u64, pub device_bytes: u64, pub scratch_bytes: u64, pub threads: u16 }
impl Budget {
    fn fits(self, max: Self) -> bool {
        self.host_bytes > 0 && self.threads > 0 && self.host_bytes <= max.host_bytes &&
        self.device_bytes <= max.device_bytes && self.scratch_bytes <= max.scratch_bytes &&
        self.threads <= max.threads
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Handle { pub id: String, pub access: Access, pub format: Format }

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Capability {
    pub operation: Operation, pub inputs: BTreeSet<Format>, pub outputs: BTreeSet<Format>,
    pub device: Device, pub precision: Precision, pub maximum: Budget,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProviderManifest {
    pub id: String, pub major: u16, pub minor: u16, pub build_sha256: String,
    pub platform: String, pub driver: String, pub capabilities: Vec<Capability>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "operation", rename_all = "snake_case", deny_unknown_fields)]
pub enum Options {
    InspectImages,
    ExtractFeatures { max_image_side: u32 },
    MatchFeatures { max_neighbors: u16 },
    SparseReconstruction,
    DenseReconstruction { max_image_side: u32, max_source_views: u16 },
    Orthorectify { target_gsd_micrometers: u64 },
    BuildSurface,
    ExportArtifact,
}
impl Options {
    pub fn operation(&self) -> Operation {
        match self {
            Self::InspectImages => Operation::InspectImages,
            Self::ExtractFeatures { .. } => Operation::ExtractFeatures,
            Self::MatchFeatures { .. } => Operation::MatchFeatures,
            Self::SparseReconstruction => Operation::SparseReconstruction,
            Self::DenseReconstruction { .. } => Operation::DenseReconstruction,
            Self::Orthorectify { .. } => Operation::Orthorectify,
            Self::BuildSurface => Operation::BuildSurface,
            Self::ExportArtifact => Operation::ExportArtifact,
        }
    }
    fn valid(&self) -> bool {
        match self {
            Self::ExtractFeatures { max_image_side } => *max_image_side > 0 && *max_image_side <= 65536,
            Self::MatchFeatures { max_neighbors } => *max_neighbors > 0 && *max_neighbors <= 4096,
            Self::DenseReconstruction { max_image_side, max_source_views } =>
                *max_image_side > 0 && *max_image_side <= 65536 && *max_source_views > 0 && *max_source_views <= 4096,
            Self::Orthorectify { target_gsd_micrometers } => *target_gsd_micrometers > 0,
            _ => true,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Request {
    pub job_id: String, pub revision_id: String, pub node_id: String,
    pub lease_id: String, pub idempotency_key: String, pub deadline_unix_ms: u64,
    pub handles: Vec<Handle>, pub dependency_sha256: Vec<String>,
    pub budget: Budget, pub device: Device, pub precision: Precision, pub options: Options,
}

/// Constructed by the trusted host after signature/license checks, never deserialized from a worker.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Approval {
    pub provider_id: String, pub build_sha256: String, pub platform: String, pub driver: String,
    pub operations: BTreeSet<Operation>,
}

/// Host-issued grants; ownership, expiry and actual backing files are checked by the host.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Grant { pub job_id: String, pub revision_id: String, pub handle: Handle }

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reject { Oversize, Malformed, Version, Unapproved, Unsupported, InvalidId, InvalidDigest, InvalidOptions, InvalidLease, InvalidHandle, Budget }

fn valid_id(s: &str) -> bool {
    !s.is_empty() && s.len() <= 128 && s.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}
fn digest(s: &str) -> bool {
    s.len() == 64 && s.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

pub fn decode_request(bytes: &[u8]) -> Result<Request, Reject> {
    if bytes.len() > MAX_CONTROL_BYTES { return Err(Reject::Oversize); }
    serde_json::from_slice(bytes).map_err(|_| Reject::Malformed)
}

pub fn negotiate(manifest: &ProviderManifest, approval: &Approval, req: &Request,
                 grants: &[Grant], active_lease: &str, now_unix_ms: u64,
                 available: Budget) -> Result<(), Reject> {
    if manifest.major != PROTOCOL_MAJOR || manifest.minor > PROTOCOL_MINOR { return Err(Reject::Version); }
    if !valid_id(&manifest.id) || !digest(&manifest.build_sha256) { return Err(Reject::Unapproved); }
    let op = req.options.operation();
    if manifest.id != approval.provider_id || manifest.build_sha256 != approval.build_sha256 ||
        manifest.platform != approval.platform || manifest.driver != approval.driver ||
        !approval.operations.contains(&op) { return Err(Reject::Unapproved); }
    for id in [&req.job_id, &req.revision_id, &req.node_id, &req.lease_id, &req.idempotency_key] {
        if !valid_id(id) { return Err(Reject::InvalidId); }
    }
    if req.deadline_unix_ms <= now_unix_ms || req.lease_id != active_lease { return Err(Reject::InvalidLease); }
    if req.dependency_sha256.len() > MAX_DEPENDENCIES || !req.dependency_sha256.iter().all(|d| digest(d)) {
        return Err(Reject::InvalidDigest);
    }
    if !req.options.valid() { return Err(Reject::InvalidOptions); }
    if req.handles.is_empty() || req.handles.len() > MAX_HANDLES { return Err(Reject::InvalidHandle); }
    let mut seen = BTreeSet::new();
    let mut reads = BTreeSet::new();
    let mut writes = BTreeSet::new();
    for h in &req.handles {
        if !valid_id(&h.id) || !seen.insert(&h.id) || !grants.iter().any(|g|
            g.job_id == req.job_id && g.revision_id == req.revision_id && g.handle == *h) {
            return Err(Reject::InvalidHandle);
        }
        match h.access { Access::Read => { reads.insert(h.format); }, Access::Write => { writes.insert(h.format); } }
    }
    if reads.is_empty() || writes.is_empty() { return Err(Reject::InvalidHandle); }
    if !req.budget.fits(available) || (req.device == Device::Cpu && req.budget.device_bytes != 0) ||
        (req.device != Device::Cpu && req.budget.device_bytes == 0) { return Err(Reject::Budget); }
    let mut supported = manifest.capabilities.iter().filter(|c| c.operation == op &&
        c.device == req.device && c.precision == req.precision &&
        reads.is_subset(&c.inputs) && writes.is_subset(&c.outputs));
    if supported.any(|c| req.budget.fits(c.maximum)) { return Ok(()); }
    if manifest.capabilities.iter().any(|c| c.operation == op && c.device == req.device &&
        c.precision == req.precision && reads.is_subset(&c.inputs) && writes.is_subset(&c.outputs)) {
        Err(Reject::Budget)
    } else { Err(Reject::Unsupported) }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture() -> (ProviderManifest, Approval, Request, Vec<Grant>, Budget) {
        let budget = Budget { host_bytes: 1024, device_bytes: 0, scratch_bytes: 2048, threads: 2 };
        let handles = vec![Handle { id: "input".into(), access: Access::Read, format: Format::Jpeg },
            Handle { id: "output".into(), access: Access::Write, format: Format::FeatureIndex }];
        let req = Request { job_id: "job".into(), revision_id: "rev".into(), node_id: "node".into(),
            lease_id: "lease".into(), idempotency_key: "once".into(), deadline_unix_ms: 100,
            handles: handles.clone(), dependency_sha256: vec!["a".repeat(64)], budget, device: Device::Cpu,
            precision: Precision::F32, options: Options::ExtractFeatures { max_image_side: 2048 } };
        let grants = handles.into_iter().map(|handle| Grant { job_id: "job".into(), revision_id: "rev".into(), handle }).collect();
        let manifest = ProviderManifest { id: "test".into(), major: 2, minor: 0, build_sha256: "a".repeat(64),
            platform: "linux-x64".into(), driver: "none".into(), capabilities: vec![Capability {
                operation: Operation::ExtractFeatures, inputs: [Format::Jpeg].into_iter().collect(),
                outputs: [Format::FeatureIndex].into_iter().collect(), device: Device::Cpu, precision: Precision::F32, maximum: budget }] };
        let approval = Approval { provider_id: manifest.id.clone(), build_sha256: manifest.build_sha256.clone(),
            platform: manifest.platform.clone(), driver: manifest.driver.clone(), operations: [Operation::ExtractFeatures].into_iter().collect() };
        (manifest, approval, req, grants, budget)
    }
    #[test] fn accepts_qualified_request_and_roundtrip() {
        let (m,a,r,g,b)=fixture();
        assert_eq!(decode_request(&serde_json::to_vec(&r).unwrap()), Ok(r.clone()));
        assert_eq!(negotiate(&m,&a,&r,&g,"lease",1,b), Ok(()));
    }
    #[test] fn rejects_incompatible_version() {
        let (mut m,a,r,g,b)=fixture(); m.major=3;
        assert_eq!(negotiate(&m,&a,&r,&g,"lease",1,b), Err(Reject::Version));
        m.major=2; m.minor=1;
        assert_eq!(negotiate(&m,&a,&r,&g,"lease",1,b), Err(Reject::Version));
    }
    #[test] fn rejects_unapproved_build() {
        let (mut m,a,r,g,b)=fixture(); m.build_sha256="b".repeat(64);
        assert_eq!(negotiate(&m,&a,&r,&g,"lease",1,b), Err(Reject::Unapproved));
    }
    #[test] fn rejects_unscoped_handle_and_wrong_revision() {
        let (m,a,mut r,g,b)=fixture(); r.handles[0].id="../secret".into();
        assert_eq!(negotiate(&m,&a,&r,&g,"lease",1,b), Err(Reject::InvalidHandle));
        r.handles[0].id="input".into(); r.revision_id="other".into();
        assert_eq!(negotiate(&m,&a,&r,&g,"lease",1,b), Err(Reject::InvalidHandle));
    }
    #[test] fn rejects_expired_lease_and_budget() {
        let (m,a,mut r,g,b)=fixture();
        assert_eq!(negotiate(&m,&a,&r,&g,"lease",100,b), Err(Reject::InvalidLease));
        assert_eq!(negotiate(&m,&a,&r,&g,"other",1,b), Err(Reject::InvalidLease));
        r.budget.host_bytes+=1;
        assert_eq!(negotiate(&m,&a,&r,&g,"lease",1,b), Err(Reject::Budget));
    }
    #[test] fn rejects_unknown_fields_and_oversize() {
        let (_,_,r,_,_)=fixture();
        let mut val=serde_json::to_value(r).unwrap(); val["shell"]=serde_json::json!("bad");
        assert_eq!(decode_request(&serde_json::to_vec(&val).unwrap()), Err(Reject::Malformed));
        assert_eq!(decode_request(&vec![0; MAX_CONTROL_BYTES+1]), Err(Reject::Oversize));
    }
    #[test] fn rejects_wrong_format_and_duplicate_handle() {
        let (m,a,mut r,mut g,b)=fixture();
        r.handles[1].format=Format::Glb; g[1].handle.format=Format::Glb;
        assert_eq!(negotiate(&m,&a,&r,&g,"lease",1,b), Err(Reject::Unsupported));
        r.handles[1].id=r.handles[0].id.clone(); g[1].handle=r.handles[1].clone();
        assert_eq!(negotiate(&m,&a,&r,&g,"lease",1,b), Err(Reject::InvalidHandle));
    }
}
