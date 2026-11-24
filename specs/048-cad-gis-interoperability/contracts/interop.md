# R048 contract extension — proposed, not public API
Depends on R031 descriptor/execution/result/lifecycle; names below are semantic commands mapped to existing registry/SDK, not new endpoints.
- inspect.capabilities: formats/CRS, selection, preview, export, entity-delta, save-document, read/write scope, artifact version/digest, limits, license.
- link.resolve / selection.focus: project-derived scope, generation, source identity and pinned revision; returns zero/one/many authorized candidates and stale/unsupported reason; transient selection no persistent writes.
- exchange.preview: binding/profile/placement revision, CAD digest/version, GIS schema/content revision, direction + selection; returns typed proposed ChangeSet or native artifact plan, loss manifest, topology/residuals, digest, expiresAt.
- exchange.commit: preview id, payload digest, expected revisions, stable operation UUID, approval receipt; server validates all again, returns durable existing operation receipt or conflict/denied/unsupported. No credential or actor claim trusted from browser.
- sync.read/ack: R029 cursor/authorized deltas; ack only after apply; invalidation signals not data authority. Same operation + digest returns same receipt; different digest rejects. Unsupported delta CAD marked stale, reviewed reload/rebase, never silently overwrites.
- conflict.resolve/revert: authorized R006 change or CAD version, expected revisions, explicit impact and reason.
- MCP: existing command allowlist + consent; preview not commit; no arbitrary JS/Rust/desktop execution.

Wire relay remains isolated, allowlisted origins/source/channel, maximum size, generation, no GIS credentials. Negotiate selection and supported deltas; preserve current protocol compatibility until reviewed. Errors: unsupported, denied, invalid, conflict, cancelled, offline, failed. Publication: stage blob, validate digest, publish metadata and outbox transactionally where possible; recovery compensates orphan staging, never fake atomicity. Feature flags default off. Contract change gate: R031 merged/reviewed and pinned compatibility test.
