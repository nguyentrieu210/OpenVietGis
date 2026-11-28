//! House-numbering persistence adapter, owned and compiled by the vertical module.
//! The Core storage crate supplies the authorized tenant transaction; the module
//! owns its queries, audit records, and transactional domain persistence.
//! No dependency on HTTP application internals or reverse dependency from Core.

// Keep the old internal catalog import path as a private adapter alias. This
// lets the existing SQL implementation move byte-for-byte without semantic drift.
use ovg_storage::catalog;

mod persistence;
pub use persistence::*;
