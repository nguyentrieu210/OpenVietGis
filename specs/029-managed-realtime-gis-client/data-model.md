# Logical data model (proposed)
Reuse R002 principals/grants, R003 source/layer/feature identifiers, R006 changesets and audit, R027 connector bindings. Actual table names/migration assigned after inventory.
- ManagedSession: id, org/project/binding/user/device, capability flags, auth epoch, expires_at, revoked_at, last_seen; no raw tokens; state active/expired/revoked/paused.
- ProjectEvent: org/project, cursor monotonic in committed order, kind (changeset/schema/catalog/visibility_reset), source_id, changeset_id, origin actor/device, committed_at; internal event metadata never returned unfiltered.
- DeltaPage: scoped events/authorized upserts and removals, next_cursor, has_more, permission_epoch, reset_required; removal for formerly visible features only or reset whole authorized cache; no unauthorized identifiers.
- CommitReceipt: operation_id, scoped actor/project, request digest, R006 changeset ID, revision and event cursor; same key+digest returns same result, different payload conflict; reuse R006 receipt infrastructure if possible.
- ManagedClientState: session, cursor, layer revisions, encrypted/OS-protected minimal cache if necessary, pending edit buffer and operation ID, state. Never project-file secrets; no offline upload without reauth/review.
- Optional FeatureLease: scope, feature, holder/session, expiry; advisory, revision checks mandatory.
- DesktopToolRequest: user/project/session, typed tool/args digest, consent status/expiry, result audit reference; no arbitrary executable strings.
## Lifecycle
Session active -> offline(read-only)/revoked/expired -> fresh auth only. Edit staged -> submitted -> accepted or denied/conflict/unknown; unknown -> query original receipt; not accepted until durable receipt. Cursor advances after atomic local apply. Expired cursor -> scoped bootstrap. ACL epoch mismatch -> immediate invalidate/reset.
