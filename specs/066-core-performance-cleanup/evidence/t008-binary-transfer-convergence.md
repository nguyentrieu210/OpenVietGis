# 066 T008 — Rust parity convergence, implementation slice

## Implemented in this slice (pending execution on a build runner)

- E07 GeoPackage and ZIP Shapefile import, inspect, confirm, and export. The existing bounded format worker handles encoding/decoding only; authorization, CRS policy, revision pins, job state, and publication stay in Rust. GeoJSON streaming remains unchanged in principle.
- E07 tests now exercise differential round trips for all three formats and archive path traversal, rather than accepting FORMAT_NOT_MIGRATED.
- E15 progress report XLSX: scoped SQL, native OOXML generation, CRC-checked ZIP container, inline strings, bounded rows/size and worker admission. Differential test compares workbook cells, not ZIP bytes.

## Remaining known implementation gaps

A static inspection of the frozen 364-operation FastAPI inventory and the mounted Rust router source identified five outstanding E15 file/document operations after this slice:

- POST /imports
- POST /documents
- POST /documents/batch
- POST /exports
- GET /proposals/{proposal_id}/packets/{kind}

Paths above are relative to the house-numbering project module. They are not replaced by stub handlers. The other apparent gaps from text-only inspection included differently named path parameters, wildcard routes, the WebSocket transport, and routes generated through closures. This static review is not a complete behavioral parity certificate.

The worker's 32 MiB binary export input, 64 MiB inspection output, 75 MiB binary artifact, and 50,000-row/32 MiB XLSX limits must be validated and reconciled against legacy behavior. No compilation, differential run, independent review, ADR acceptance, or live cutover is claimed. The final T008/T045/T047 gates remain open.

## E15 file-contract implementation follow-up (pending verification)

All five outstanding E15 route registrations now have implementations, not placeholder handlers. Scoped Rust transactions handle evidence staging/replay, document issue/reissue, batch issue, public/private export authorization/auditing, and proposal packet state checks. The isolated codec adapter renders legacy PDF/DOCX/QR/XLSX layouts and parses/writes CSV/XLSX/GeoJSON/DXF/Shapefile without DB or HTTP access. New differential cases exercise successful and negative flows, binary headers, artifact integrity and import idempotence. The codec remains Python as a format engine, not a legacy API/control-plane fallback. Bounded resource and artifact limits are intentional and must be verified against contract expectations. T008 remains open until compilation, integration and remaining runtime parity evidence are available.
