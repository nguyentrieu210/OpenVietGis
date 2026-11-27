//! Isolated official `house_numbering` vertical module depending ONLY on `ovg-module-sdk` and `ovg-core`.

pub mod address_review;
pub mod assignment;
pub mod domain;
pub mod module;
pub mod numbering;
pub mod planning;
pub mod prefix;
pub mod recovery;
pub mod route_import;

pub use module::HouseNumberingModule;
pub use numbering::{generate_street_numbers, HouseSide, NumberingProposal, NumberingRule};
pub use prefix::{address_chain, canonical_prefix};
