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
