# Project CAD document vault — integration checkpoint

Base: /api/v1/organizations/{organization_id}/projects/{project_id}/cad/documents.
Human session; existing project authorization and GIS access guard apply; explicit
unrestricted project scope is required. All writes require project.edit and CSRF;
reads require project.read. Restricted GIS users and service principals are denied.

| Method | Suffix | Result |
|---|---|---|
| GET | (root) | items: latest version of each document |
| POST | (root) | multipart file; new immutable version 1 (201) |
| GET | /{document_id}/versions | items: versions descending |
| POST | /{document_id}/versions | multipart file and expected_version; new immutable version (201) |
| GET | /{document_id}/versions/{version}/download | original bytes, attachment, no-store, verified SHA-256 |

Metadata: id, version, filename, format (dwg/dxf), sha256, byte_size, created_at.
Storage keys are internal and never returned. Header validation is a bounded format
screen, not a complete DWG/DXF parser or safety guarantee. Max 25 MiB per file, 100
documents, 20 versions per document, 500 MiB aggregate per project. No delete or
overwrite endpoint. Stale expected_version returns 409. Version format cannot change.
Transactional project audit, RLS FORCE, read/insert only runtime grants. The local
object store is immutable by unique object key, and checks digest on download.

## 3D integration

Only feature-read-authorized polygon rows explicitly fetched in EPSG:4326 are
eligible. User selects a positive numeric attribute interpreted as height in metres;
flat relative base is explicitly zero, not terrain or orthometric elevation. 3D is
read-only, bounded by adapter limits and has no remote basemap. GIS camera and
edit state remain mounted; GIS commands and partner bridge disabled during alternate
mode. Restricted/unknown permissions close alternate modes; data snapshots refresh
by returning to GIS. CAD mode is a vault, not the remote CAD editor.

## Promotion gate

CAD engine must be reviewed, isolated and use a verified capability/protocol before
direct edit/import or GIS overlay. No remote editor is proxied into authenticated GIS
origin. Production backup and restore must include database and artifact store
together; orphan cleanup, malware/parser isolation, request ingress limits, full
regression/convergence and deployment planning are not completed in this checkpoint.
