# Architecture before / target
Before: apps/web-next and apps/web; apps/api-rust and apps/api; apps/mcp-rust and apps/mcp; root scripts still use uv; multiple infrastructure profiles and workflows.
Target (conditional after gates):
- apps/web-next: single SPA; shell, features/gis, features/cad, features/three-d, features/auth, shared UI/runtime boundaries. Retain path name unless migration benefit proven.
- apps/api-rust: HTTP adapters + domain-oriented modules; reusable code in existing crates/ovg-{core,storage,spatial,sync,processing,module-sdk}; no new crates without dependency evidence.
- apps/mcp-rust: dedicated protocol adapter; evaluate workspace membership; reuse policy crate.
- packages/{client-sdk,plugin-sdk,spatial-workspace}: public interfaces and shared engines only.
- modules/{house_numbering,land_asset}: depend on public core interfaces, never reverse.
- integrations/qgis_plugin and microstation-v8i-bridge: preserve native client language/protocol.
- infra/{dev,prod,rust-platform,cad-relay}: converge active profiles, keep documented rollback only as needed.
- tools/, tests/, specs/, docs/: separate active checks, fixtures, archives and immutable evidence; no mass moving old specs that breaks links.
Enforce import/dependency rules with existing architecture checks or equivalents; move in vertical slices, keep compatibility imports only until consumers migrate. No separate local GIS fork, no second router. Runtime graph: browser shell -> active engine -> scoped SDK -> Rust API -> storage/processing; realtime revisions invalidate scoped caches. Public edge handles only approved public immutable assets.

## Current source closure

The canonical frontend is apps/web-next. The old SPA was retired with route,
OAuth, offline and consumer evidence in evidence/implementation-frontend-retirement.md.
Python remains for migrations, embedded adapters and independent behavioral
oracles. ADR-004 stays Accepted and ADR-015 stays Proposed; frontend cleanup
does not authorize backend retirement.

The API owns tile_cache. Storage validates current layer/source/revisions/render
support before producing a cache scope. The handler obtains that proof before a
hit and again after waiting. Metrics have no tenant/actor labels. infra/release
verifies image identity and health and records build timings; these operations
neither deploy services nor grant release approval.
