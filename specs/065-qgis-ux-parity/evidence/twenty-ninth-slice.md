# Twenty-ninth implementation slice (GitHub only)
Base 1e0aede326d3cf3d17223b9d0930dbbf0d24e86a.
- COLLAB-09: offline sync now validates package ID, group operation ID, count, atomic applied/conflict status and every identified operation receipt against the frozen outbox. Legacy attempted operations without client IDs retain original wire intent. HTTP 409/422 no longer fabricate a terminal rejected receipt; retain exact batch for explicit reconciliation/replay.
- IndexedDB CAS additionally blocks changing an attempted group or stored receipt, reopening a locked package, and changing identity/checksum/expiry metadata. Added expiry/checksum/byte-limit guards.
- Scoped offline page remounts by project; account/connection generation and in-flight guards prevent late sync/download UI publication or device unlock after a session changes. Capability check includes actor authorization.
- Added receipt and immutable-transition tests. Direct GitHub source only, not compiled or executed. No new offline end-to-end, service worker, device or paired QGIS acceptance. No deployment; matrix unchanged.

Follow-up: initial feature paging validates org/project/layer/source/revisions/CRS/geometry flag, detects duplicate FIDs and non-advancing cursors; a failed layer scan removes its partial rows. Cache and incremental delta require complete unique populations, bound age/size and check scope metadata. Account change invalidates initial load. No runtime or browser validation of these new guards; cache remains online-authorized, not a general offline fallback.
