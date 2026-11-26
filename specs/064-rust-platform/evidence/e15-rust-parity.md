# E15 Rust-native house numbering — parity evidence

Branch `064-e15-rust-native` (off `064-convergence`). Owner decision 2026-11-26 moves the
E15 control plane to Rust; FastAPI stays the oracle and rollback path for routes not yet
accepted. Per-route status lives in `contracts/engines/e15.json` (`rust_status`,
`rust_note`): 48 native, 6 retained Python engines, 18 pending.

## Layout (Principle I)
- Pure rules: `modules/house_numbering/rust/src/domain.rs` (number parse/format,
  `suggest_number`, `locate_on_route`, reconciliation class, certificate match) and
  `planning.rs` (batch route plan, numbering audit, street-tree collisions).
- Storage: `crates/ovg-storage/src/house_numbering/` (hierarchy, buildings, proposals,
  field, records, numbering). No dependency on the module crate; module rules are passed in
  as closures where storage needs them (reconciliation class, tree collisions).
- HTTP: `apps/api-rust/src/house_numbering/`, compiled only with the `house-numbering`
  feature; `--no-default-features` mounts no module routes.

## Contract notes
- Every route opens the RLS-bound project scope, then checks the official activation against
  the shipped manifest (same 404 as Python). `prepare("manage")` routes return the Core
  `{"detail": "missing permission: project.manage"}` 403, including mid-operation checks
  (delegated survey recorders, private export downloads).
- Errors: `PlatformDomainError` → `{code, message[, details]}`; request validation →
  the sanitized `{"detail": [...]}` 422; SQLSTATE OV409 guards → 409 `DATA_LOCKED`.
- Timestamps keep Python's six-digit microseconds; rows FastAPI serializes through
  Pydantic use `Z`, `_wire` rows keep `+00:00`. `numeric` columns stay scale-preserving
  strings and `double precision` columns stay JSON floats.
- Interchangeability: the route snapshot hash uses Python `json.dumps(sort_keys=True)`
  encoding and the survey payload fingerprint the compact form, so previews and offline
  replays written by either runtime are recognized by the other (asserted in parity).

## Parity gate
`tests/rust-parity/e15_full_parity.py` runs every case once per runtime against its own
fixtures in one disposable PostGIS database and compares normalized bodies (UUIDs,
timestamps, sha256 tokens), status codes and, for downloads, bytes plus content headers.
The PostGIS parity job also runs the existing `e15_house_numbering_reads.py` and
`e15_house_numbering_writes.py`.

## Retained Python engines (thin adapters behind the same module scope)
Proposal packets (PDF/XLSX/QR), progress workbook, document issue/batch rendering, export
encoding (CSV/GeoJSON/DXF/Shapefile) and multipart evidence import. Rust already serves the
JSON/history/download sides of these features, so tenant authorization never depends on the
retained adapters.

## Pending
Route-tree import (preview/apply/stage-geometry), address verification and verified links,
building assignment with catalog/prefix recovery and cluster correction, survey photo upload
(multipart) and the service-credential integration lookup (needs E14's
`ProjectRead::begin_service`). These stay on FastAPI until ported with parity.
