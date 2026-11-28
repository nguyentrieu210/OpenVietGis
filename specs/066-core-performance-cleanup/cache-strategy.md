# Cache policy
| Class | Storage | Key / validation | Invalidation |
|---|---|---|---|
| Hashed JS/CSS/WASM/assets | HTTP/browser/CDN | content hash, build ID; immutable only when content-addressed | publish new path, retain prior assets through rollback window |
| HTML shell/SW | browser revalidation | release ID, no stale app shell | revalidate; preserve offline package drafts through migration |
| Public tiles | provider-permitted cache | provider, style, z/x/y, CRS, data version | provider license/TTL and revision |
| Authorized catalog/query/tiles | process memory, scoped app cache if approved | tenant, actor/effective permission revision, project, resource, query, CRS/style, data revision | auth check first even on hit/304; mutation, revocation, session epoch, logout |
| Offline field | existing identity-bound store | user/tenant/project/package version | explicit revocation/expiry, preserve unsynced changes via approved recovery |
| Spatial computation | bounded backend cache | scoped inputs, revision, algorithm/CRS/version | committed revision/event; no cross-tenant reuse without proof |
Never put private API, signed URLs, tokens, authenticated HTML or tenant tiles into shared edge/SW cache. Use no-store for sensitive responses. Auth failures fail closed. No raw PII in keys/logs. TTL is a fallback, not a permission mechanism.
Use bounded byte budget, LRU, single-flight, jitter, cancellation, negative-cache safety, per-tenant quota. Proposed initial TTL: metadata 15–60s subject to revision, immutable assets 1 year, no stale private data on revocation. Use existing components first. Distributed cache only if measured need. Event: DB commit -> durable outbox/revision -> invalidation -> client delta/revalidation; detect gaps, scoped snapshot only when required. Multiple tabs coordinate logout/revocation; unacknowledged local drafts never discarded. Test cold/warm/stale, eviction, stampede, release mismatch, two tenants and role revocation.

## Private MVT cache source

The native 4326 route has 64 entries / 32 MiB, 60-second TTL and 64 singleflight
keys. Keys bind actor, tenant, project, layer, source, revisions and tile address.
Current project/RLS/layer access, source readiness, pins and render support are
checked before every lookup. Feature-restricted GIS scopes bypass the cache.
Waiters release the DB transaction and repeat authorization after completion or
a bounded wait. Immutable bytes are shared; cancelled leaders wake all waiters.

Rust tests cover memory/entry/flight limits, delayed polling, cancellation,
identity/source/revision/address isolation, expiration and metric privacy.
The final candidate must also pass tests/rust-parity/e05_cache_security.py;
source review alone does not prove HTTP revocation or current-pin parity.
