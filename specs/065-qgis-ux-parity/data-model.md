# Data Model — UX state and evidence
Proposed logical models; reuse current storage and API rather than automatically introducing DB tables.

| Entity | Required fields | Persistence / security |
|---|---|---|
| WorkspaceScope | account_id, organization_id, project_id, mode, active_resource_id, revision | Resolved from authenticated context; validate permissions on restore |
| UserUiProfile | schema_version, account scope, locale, shortcuts, density, panel prefs | Personal; scoped localStorage for non-sensitive values; cross-device optional behind authorized endpoint |
| DockLayout | profile_version, panels, region, order, stack_id, dimensions, hidden, floating_bounds | Personal per mode/project; clamp geometry; migrate old keys, reset available |
| CommandDescriptor | stable id, labels, aliases, category, modes, input contract, capability, disabled reason, execute handler | Static registry; handler uses authoritative server authorization |
| MapConfiguration | project_id, revision, layer_refs, groups, style refs, themes, visibility, render order | Shared versioned map config; explicit commit and conflict checking |
| LayerContext | source_id, layer_id, schema_revision, content_revision, source/storage/display CRS, capabilities | Authoritative server; invalidate stale client selection |
| SelectionState | scope, layer_id, revision, selected ids or query token, count | Ephemeral or bounded personal state; query server for large selections |
| DraftChange | operation_id, owner, scope, base_revision, changes, dirty state | Protected offline cache if supported; not general localStorage, never auto-replay |
| JobReceipt | job_id, input revisions, status, progress, output refs, timestamps, error, retry/cancel capability | Durable existing job API; scope checks and idempotency |
| EvidenceRecord | matrix_id, baseline, fixture checksum, version, test IDs, outcome, artifact refs, limitations | Repo docs/CI artifacts; synthetic fixtures only |

## State transitions
Draft: clean → dirty → validating → saving → saved/conflict/failed. Cancel discards only the explicitly selected draft; persisted changes use versioned revert. Pending commands cannot replay after reload. Restore preferences only after valid identity/project; don't force mode switch that bypasses dirty guard.
Job: queued → running → succeeded/failed/canceled; partial outcomes visible. Retry creates/reuses operation receipt according to API contract. UI reports real progress or indeterminate, never simulated.
