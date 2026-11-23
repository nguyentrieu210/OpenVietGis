# OpenVietGIS ↔ MicroStation V8i (experimental bridge)

This is a **read-only contract + COM probe**, not a deployed cadastral synchronization product.

- `bridge.py probe`: Windows-only attach to a **running** V8i COM session; shows file basename/model, never starts, edits or saves V8i.
- `bridge.py scan --drawing-id drawing1 --model-id model1 --source-crs EPSG:XXXX --output snapshot.json`: experimental read-only scan of **active model** for simple Line, LineString and Shape entities, writing a **new local file** (will not overwrite). Requires licensed V8i, pywin32, explicit source CRS, and user verification of COM coordinate units/control points before cadastral use. Counts unsupported CAD entities. Does not traverse reference attachments or join external parcel attribute databases.
- `bridge.py validate <snapshot.json>`: source-CRS native geometry validation, stable SHA-256 digest. Never treats projected VN-2000 as WGS84.
- `bridge.py diff <old.json> <new.json>`: previews upserts and human-review-only deletion candidates; never sends writes to server.
- `bridge_contract.py`: portable exchange data contract, independently testable on Linux.

No Bentley SDK or binary is bundled. No credentials, network transfer, direct DGN parser, cadastral field mapper, **verified Windows COM scan**, PostGIS sync, or bidirectional editing is provided yet. Linux fake-COM tests do not prove real V8i interoperability. The server capability endpoint is project-authenticated but never pairs devices and never writes data.

The future connector must use explicit device pairing and project-scoped authorization; the R010 External Sources API remains read-only and must not be repurposed as a write ingress.

See `../../specs/025-microstation-v8i-bridge` for feature specifications, QA gates and task assignments.
