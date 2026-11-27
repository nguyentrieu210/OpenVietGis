# ADR-015: Rust-native platform migration
**Status**: Proposed | **Date**: 2026-11-25
## Context
Existing Python/FastAPI remains the deployed/reference architecture. Owner requests a new Rust-first core to improve throughput and resource use. No performance gains assumed without measurement.
## Decision
Propose Rust modular monolith (Axum/Tokio, bounded Rayon, SQLx/PostGIS, native geospatial adapters); separate tile delivery, durable incremental sync, portable WASM only where tested. Retain existing clients and database. Prefer stable contracts and a staged replacement over line-by-line translation or microservices.
## Alternatives
Optimize existing Python, native WKB extension (Spec 063), Actix, wholesale rewrite including renderers/database. Existing optimization is a useful baseline; no evidence that replacing renderers/database is beneficial.
## Consequences and gates
Higher initial engineering cost, native dependency/CRS risk, parity and operational testing mandatory. Proposed eventual replacement of ADR-004 only after approval; it remains Accepted now. No direct public tenant tile sources, no irreversible migration or release before review, benchmark and rollback. See Spec 064.
