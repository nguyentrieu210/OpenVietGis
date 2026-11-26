//! Isolated official `house_numbering` vertical module depending ONLY on `ovg-module-sdk` and `ovg-core`.

pub mod domain;
pub mod module;
pub mod numbering;
pub mod planning;
pub mod prefix;

pub use module::HouseNumberingModule;
pub use numbering::{generate_street_numbers, HouseSide, NumberingProposal, NumberingRule};
pub use prefix::{address_chain, canonical_prefix};
