# Implementation plan
Reuse React, MapLibre GPU symbols, bounded workers, Rust API/PostGIS and existing capability contracts. No new JS dependencies unless justified. Build on approved runner; Milion deploy-only.
- Wave A E01–E04: state helpers, keyboard and quick controls, batches and context organization.
- Wave B E05–E10: editing/presentation contracts, cached viewport anchors, collision and priority, label overrides and rules.
- Wave C E11–E14: persistent order, style/theme portability, tile parity, interoperability. Server attribute-dependent tile support must honor existing field authorization and revision pins. No unreviewed database migration.
- Wave D E15: complete regression, desktop/browser keyboard, cross-scope, large dataset and physical GPU performance evidence, source convergence and dev deploy with rollback.
Existing state is not replaced by duplicate sources of truth. New personal UI state uses account/org/project preference keys. Shared persistence uses existing map revision APIs or explicit versioned contract migration. Separate source identity from layer instance identity.
