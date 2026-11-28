# Wave 0 implementation checkpoint (partial)
Baseline SPEC 075 commit 6bb971b. All F001–F012 stay open until acceptance.
## Reuse inventory
- Canonical registry: Rust routes in apps/api-rust/src/extensions/{manifests,activations,records,marketplace,marketplace_developers,partner_platform,validation}.rs; storage in crates/ovg-storage/src/extensions. Existing RLS extensions tables and history in Alembic migrations. No new registry or migration in this slice.
- Official modules: crates/ovg-module-sdk and modules/house_numbering/rust, SPEC 039, existing activation pins. Partner v1 iframe SDK: packages/plugin-sdk and packages/client-sdk/src/external-plugins.ts; Web host: apps/web-next/src/ExternalPluginHub.tsx, plugin-lifecycle.ts, plugin-ui-registry.ts, PluginWorkspace.tsx, SPEC 071.
- Mobile and Desktop: reconcile active SPEC 073/074 implementation before shipping declarative hosts. Do not replace existing SDK.
## Implemented slice
- Additive V2 signed contribution verification (publisher plus independent operator, trusted manifest digest, asset digests, host/API compatibility, permissions and catalog epoch); exact dependency lock validation with bounded deterministic topological order. No v1 signature changes.
- Typed declarative document and preflight checks; no dynamic code execution. Preflight caller must bind actual server-side grant and confirmation; command dispatch, job quota, permission rechecks, persistent receipts and audit still open.
- Existing signed v1 marketplace search and pagination API plus Web filter and SDK functions.
- Opt-in verified v2 metadata endpoint using existing v1 signed catalog trust and a separate operator-reviewed catalog file. Set OPENVIETGIS_MARKETPLACE_V2_CATALOG_PATH and OPENVIETGIS_MARKETPLACE_V2_EPOCH together; default disabled. Publisher key_id is lowercase SHA256 of raw 32-byte Ed25519 public key. Operator signs canonical {schema:"openvietgis.review.v2",envelope_digest:<SHA256 of canonical typed envelope>,epoch:<positive JS-safe integer>}. Contribution/assets base64, publisher/operator signatures base64. Epoch pinned by operator config; no persistent monotonic watermark yet. Runtime_available is always false. No install, activation or delivery of executable payloads.
- Python developer signing CLI; does not replace moderation or server verification.
## Remaining
F001 mapping of 150 packages, F002 exact compatibility and golden vectors, F003 persistent key rotation/revocation/metadata, F004 full dependency selection/transactional lock and lifecycle, F005 actual dispatch and isolation/audit/resource quotas, F006 native/Web declarative renderer, F007 remaining SDKs and samples, F008 marketplace moderation/versioning/private catalog/rollback, F009 payments and entitlement, F010 migrations and recovery, F011 end-to-end reference modules, F012 consolidated integration and review. No live deployment.

## Verified on code commit `430907e86340d3b6946db415c89af8056a6d86a0`
- All 15 changed files matched the fetched GitHub blob hashes on the separate VPS development checkout.
- Rust 1.85.1 locked/offline: ovg-module-sdk lib 11/11; openvietgis-api-rust no-default-features lib 103/103 on an unprivileged Linux account. Initial root-only run failed the existing offline worker policy; running the exact built binary as `nobody` passed. No production changes.
- TypeScript client-sdk and web-next typecheck passed; Node 24.22.2 was used locally (project expects Node 26, so exact supported toolchain remains a CI gate). Python signing CLI syntax compiled. `git diff --check` and changed Rust file formatting passed.
- No full CI, real PostGIS integration, payment sandbox end-to-end, signed Web/mobile/desktop runtime activation, independent security review, or release gate claimed. F001–F012 remain open.
