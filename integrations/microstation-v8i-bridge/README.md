# OpenVietGIS ↔ MicroStation V8i (experimental bridge)

This is a **read-only contract + COM probe**, not a deployed cadastral synchronization product.

- `bridge.py probe`: Windows-only attach to a **running** V8i COM session; shows file basename/model, never starts, edits or saves V8i.
- `bridge.py validate <snapshot.json>`: source-CRS native geometry validation, stable SHA-256 digest. Never treats projected VN-2000 as WGS84.
- `bridge.py diff <old.json> <new.json>`: previews upserts and human-review-only deletion candidates; never sends writes to server.
- `bridge_contract.py`: portable exchange data contract, independently testable on Linux.

No Bentley SDK or binary is bundled. No credentials, network transfer, direct DGN parser, cadastral field mapper, COM element scan, PostGIS sync, or bidirectional editing is provided yet.

The future connector must use explicit device pairing and project-scoped authorization; the R010 External Sources API remains read-only and must not be repurposed as a write ingress.

See `../../specs/025-microstation-v8i-bridge` for feature specifications, QA gates and task assignments.
