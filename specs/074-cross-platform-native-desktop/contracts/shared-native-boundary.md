# Shared native boundary — proposed, pending W0 review

## Ownership
SPEC 073 owns portable runtime, local schema, journal, managed sync and Mobile FFI until integration owner approves a shared extraction. SPEC 074 owns Desktop shell, IPC, platform UX, installer, updater and OS qualification. One owner for any shared Cargo manifest, schema migration or ABI. Do not introduce parallel authoritative local stores or a new sync protocol.

## Data invariants
Canonical UUID, explicit CRS/axis/unit, geometry dimension capability, revision, stable operation ID and schema version. No implicit reprojection or coordinate rounding. Unsupported dimensions fail explicitly. Feature mutation + journal in one atomic transaction; receipt and cursor atomic. Standalone standard export separate from managed private data. External edits cannot bypass managed journal. Enforce lease, tenant and ACL epoch when applicable. Server is authority for connected mutations and permissions; local standalone data remains local until explicit user action.

## Transport
Desktop IPC messages must have versioned typed input/output, allowlisted operation and resource handle, explicit limits, cancellation and structured error. Never expose arbitrary SQL, paths, shells or credentials. File handles acquired through scoped OS picker; origin and window authorization enforced. No direct cloud DB connection. Reuse HTTP API unchanged unless additive versioned change reviewed.

## Qualification
Shared golden vectors and CRS fixtures, cross-client workspace and recovery tests, isolated server conflict/receipt/revocation tests. Cross-platform schema upgrades must be forward-safe or fail without modifying source. Desktop signing, updater, dependency graph and WebView qualification are separate evidence. Do not advertise an OS, format, algorithm or offline capability that lacks acceptance.

## Revision 2 read plane
The [local data protocol](local-data-protocol.md) is read-only, revision scoped and bounded; it never becomes a second storage, sync or authorization system. Mutations and jobs stay on typed IPC. Shared source contracts remain SPEC 073 owned; Desktop render cache and protocol adapter are SPEC 074 owned. [Performance qualification](performance.md) is required per advertised OS.
