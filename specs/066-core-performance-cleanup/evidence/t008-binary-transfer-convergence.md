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
