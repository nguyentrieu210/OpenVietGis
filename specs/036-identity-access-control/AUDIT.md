# Identity and access hardening — implementation evidence
Baseline 635c56a. Scope: managed GIS transport and desktop session authorization. No changes to production grants or database.

## Policy matrix
| Operation | Actor owner | Project manager other actor | Other project member |
|---|---|---|---|
| List sessions | Own | All authorized project sessions | Own |
| Inspect session | Own | Allowed | Denied |
| Bootstrap/delta/commit | Current authorization and own session | Denied | Denied |
| Subscribe to managed events | Own session | Denied | Denied |
| Queue desktop command | Own, tool permission and consent flow | Allowed with tool permission | Denied |
| Read or respond to desktop command | Own only | Denied | Denied |
| Revoke session | Own | Allowed | Denied |
| Bump project epoch | Only with project.manage | Allowed | Denied |

Session ownership uses authenticated actor type and ID, not caller-provided ID. Project permission checks remain authoritative. Current epoch required for bootstrap, commit, stream admission and desktop commands; delta returns reset on stale epoch. Receipt lookup includes actor type as well as ID. Scoped delta no longer publishes historical touched IDs when source is visible but objects may be hidden: reset and reload filtered snapshot. This intentionally sacrifices incremental scoped refresh until safe event-time visibility semantics are verified.

The SDK and web QGIS manager now default to a single-session revoke. UI acknowledges server revocation, not an unverified desktop state.

## Evidence
- 62 identity/authorization/permission registry unit and contract tests passed.
- Four R029 integration tests passed in fresh loopback-only PostGIS 18-3.7 container, migrated through 0042 with repository test runtime-role initialization. Existing pairing and commit/delta/receipt regression plus new actor/manager isolation and epoch authority, scoped bootstrap and hidden-ID delta cases passed. Synthetic test data only. Initial fixture startup failed because the runtime role password was not initialized; corrected using repository test-only SQL, not production.
- Six focused SDK HTTP/managed-sync tests passed; web-next build, Ruff, architecture and documentation checks passed.
- No migration added, no live deployment, no account/grant mutation.

## Remaining security and runtime obligations
- Actor ownership is not per-credential-instance binding. Same service principal with multiple active credentials, paired binding scope beyond session creation, credential revoke/reissue and binding lifecycle require a dedicated cross-client audit.
- SSE admission is guarded, but ongoing stream recheck currently checks session row state/expiry, not full current credential, role, ACL or epoch. Delta/commit requests still reauthorize. No claim of instant revocation of every open stream or local cached copy.
- GIS ACL change epoch propagation and scoped reconnect behavior require real Windows QGIS tests. The scoped reset fallback is conservative and can require frequent reconnects; do not claim optimized realtime.
- Manager enqueue retains existing desktop allowlist and local consent; no server proof of physical consent or complete tool execution. Native QGIS and MCP end-to-end acceptance still open.
- Full independent security review, RLS coverage and release certification remain required.
