# Local data protocol — v1 proposal
Status: mandatory contract for qualification, not implemented. Logical ovg:// names are application resource identifiers, not a promise of identical WebView URL syntax on all OSes.

## Read plane
Canonical logical examples (UUID identifiers; opaque session grant supplied through trusted host context, never a secret in URLs):
- ovg://v1/workspaces/{workspace}/layers/{layer}/revisions/{revision}/tilejson
- ovg://v1/workspaces/{workspace}/layers/{layer}/revisions/{revision}/tiles/{z}/{x}/{y}.mvt
- ovg://v1/workspaces/{workspace}/rasters/{layer}/revisions/{revision}/tiles/{z}/{x}/{y}.png
- ovg://v1/workspaces/{workspace}/cad/{document}/revisions/{revision}/chunks/{chunk}
- ovg://v1/workspaces/{workspace}/pointcloud/{layer}/revisions/{revision}/nodes/{node}
- ovg://v1/workspaces/{workspace}/assets/{asset}/revisions/{revision}/content

Allowlist each capability and exact path grammar; bounded ASCII IDs, canonical UUIDs, revision tokens and integer XYZ with z<=22 and x,y<2^z unless qualified provider defines stricter limits. No dot segments, encoded separators, duplicate query keys, external URLs, raw paths or unknown fields. Strictly parse once and authorize the canonical request; reject ambiguous encodings. GET/HEAD only; range support limited to qualified assets, single validated bounded byte range. Typed attributes/query/picking use capped pages and opaque revision-bound cursors. No filesystem traversal from URL or arbitrary SQL.

Control commands are separate typed IPC: open workspace by scoped file grant, list capabilities, edit with expected revision, submit/cancel job, close workspace. All mutations use shared canonical transactions and return structured errors. No mutation through resource GET.

## Authority and routing
Each request checks trusted window/origin, active runtime session, workspace/layer membership, revision, granted operation and managed lease/ACL epoch. Resource IDs are not authorization. Distinguish standalone local from managed/connected scope; revoke grants and purge protected cache on close/logout/revocation. Do not expose unmanaged fallback to protected content. Revalidate before returning a long-running response. No raw DB access, arbitrary network fetch, shell or general filesystem capability in renderer. No local data egress without explicit user action/approved sync.

Use MapLibre addProtocol and qualified Cesium resource adapter; map logical IDs to Tauri platform-safe transport. Prefer application-local custom protocol or scoped typed binary IPC where supported. No listening socket by default. If qualification requires loopback, separate threat review: random port, per-session scoped short-lived credentials, strict origin/host checks, no wildcard CORS, no external bind, no token logging, DNS rebinding/CSRF tests. No claim of support before OS-specific test.

## Response and scheduling
MVT/PNG/qualified binary with explicit MIME, lengths and no unsafe redirects. Stable ETag/content revision; cache key includes workspace, source, revision, format, render parameters, authorization scope, CRS and provider version. Protected cache private, no shared HTTP cache; bound memory/disk by bytes and purge at lease expiry. Render simplification never overwrites canonical geometry.
Initial limits (tune from evidence): 8 MiB decompressed tile/resource chunk, 1,000 rows and 2 MiB per attribute page, 16 concurrent reads per workspace, 64 pending jobs, 256 MiB process tile cache. No unbounded queues; prioritize visible viewport, deduplicate and cancel stale requests; structured BUSY/TOO_LARGE/NOT_AVAILABLE; never silently truncate features. CPU/file work off UI thread. Streams and response buffers respect cancellation and limits before allocation/decompression.
## Tests
Golden MVT/raster tile decode, CRS oracle, XYZ edge, no full layer transfer, revision invalidation, range fuzzing, revoked scope, close/cancel race, cross-window access, malicious encoded paths, bounded resource exhaustion, hostile fixture, unchanged browser HTTP and external-network denial. Run on WebView2/WKWebView/WebKitGTK where qualified.
