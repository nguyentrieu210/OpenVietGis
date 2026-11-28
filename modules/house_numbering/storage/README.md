# House Numbering storage adapter

This crate owns SQL and transactional persistence for the official house-numbering module. The pure numbering/domain crate does not depend on SQLx or storage.

Dependency direction:

    openvietgis-api-rust (optional house-numbering feature)
      -> ovg-module-house-numbering-storage
          -> ovg-storage / ovg-core
      -> ovg-module-house-numbering
          -> ovg-module-sdk / ovg-core

Core has no dependency on either house-numbering crate. The HTTP adapter obtains a verified unrestricted project transaction, verifies activation/permissions, invokes storage and commits. Storage must not open a new unscoped connection or commit on behalf of the HTTP caller.

Use Core module_ports for linked-feature validation and authenticated project audit. Domain-specific PostGIS joins still exist in this adapter; these are intentionally preserved pending stable spatial query services and parity tests. Existing schema/migration ownership remains under modules/house_numbering/migrations. Moving this crate does not require any database migration.

The Rust API must compile with both default features and --no-default-features; the latter excludes both module crates from its dependency graph. Architecture checks reject Core-to-module dependencies and module-to-HTTP application imports.
