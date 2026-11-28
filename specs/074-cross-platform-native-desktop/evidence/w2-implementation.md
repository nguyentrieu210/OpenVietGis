# W2 implementation checkpoint

Scope: desktop source work against the existing temporary exact PR #103 pin 2acf58d897bb093966f18639d087ef904a72a720; PR #103 remains unmerged. W0/W1 acceptance not presumed. No production changes.

| Task | Implemented candidate | Remaining qualification/scope |
|---|---|---|
| T009 | Shared LocalStore app-managed GeoPackage; no duplicate database; actual native create/open/edit | Independent external GeoPackage import/roundtrip; arbitrary CRS and multi-layer support |
| T010 | Native file dialog for bounded GeoJSON import, GeoJSON export and no-clobber GPKG snapshot; attribute paging | General file grants, persistent project metadata, arbitrary GPKG open and full interoperability |
| T011 | Shared draft validator + atomic commit_edit, revision-checked attribute changes/deletion, persisted history_step, viewport display of point/line/polygon | Existing-geometry reshape/vertex manipulation, schema-driven form and end-to-end WebView test |
| T012 | Shared checkpoint and snapshot; host test covers create/edit/undo/redo/reopen/stale revision/no-clobber | Restore/import, migration, forced kill, full disk, corruption, aggregate quota and real OS snapshot validation |
| T013 | Standalone only; managed/cloud false; no fallback or automatic upload; opaque workspace handle; native dialog never exposes paths | Protected storage authorization remains blocked on 073 |
| T036 | Bounded binary viewport GeoJSON using shared RTree; PNG MBTiles raster read-only adapter and logical ovg:// XYZ route through typed binary IPC | Actual MVT generation/TileJSON, native custom-scheme transport qualification, pinned revision for external archives, general raster/CRS |
| T037 | Attribute pages and feature details; revision/fid as decimal strings to avoid JS precision loss | CAD/point cloud/3D adapters; revision-bound opaque pagination cursor |
| T038 | Existing 16-operation IPC bound; one raster per workspace; bounded viewport, PNG bytes/dimensions; one pending viewport refresh and stale result suppression; AbortSignal checked around raster reads | Byte-bounded cache integration, immutable revision archive, priority/cancel in-flight native tile read, close/revocation stress; no claims of full LOD/streaming |

Native resource route authorizes only an open workspace handle and matching current archive UUID, revision 0; no raw path. Raster PNG only, header signature and dimensions checked, no decoder qualification. Missing/unsupported routes fail explicitly. External MBTiles is session-only/read-only; it must not be edited externally during use. No durable render cache. Attribution shown as text, not injected HTML. MapLibre uses a custom logical protocol handler mapping to typed binary IPC; the WebView-native custom scheme remains fail closed. Vector viewport uses a bounded collection (512 features and approximately 1 MiB at source), explicitly reports truncation; this is not MVT streaming or large dataset acceptance. GeoJSON import at most 16 MiB/4096 features; export bounded by existing 073 implementation. Native dialog capability used in Rust, not broad frontend file access.

Tests added: host lifecycle with real local edit, extent query, attribute revision, undo/redo, snapshot no-clobber, reopen persistence; frontend binary viewport decode and limit. W1 workflow covers the changed Desktop source and frontend tests. No runtime/CI PASS claimed; all task checkboxes remain open until evidence. Windows/macOS/Linux launch and full W2 acceptance outstanding.
