# Native processing and persistent relations implementation

Status: implementation only; no acceptance waiver. Tests added but not verified at this revision.

- Native registry expands from 5 to 8 algorithms; jobs persist parameters and output CRS.
- Union: two Polygon layers, intersection and both unmatched portions, source and overlay attributes and provenance.
- Reproject: approved EPSG:4326/3857 pipeline, source attributes and schema preserved. VN-2000 requires separately verified transformation provenance; deliberately not advertised by this implementation.
- Spatial join: seven predicates, left/inner joins, deterministic first/all match, both schemas and source lineage.
- Processing workbench, Toolbox command registry, menus, batch and model designer use the same algorithm contract.
- Project relation configuration: permission project.manage, project-scoped RLS, same-typed field pins, foreign keys to field definitions, audit on creation/removal.
- Canonical changesets check both sides in the same transaction before publishing; duplicate target keys rejected in many-to-one, NULL allowed, unmatched non-NULL source keys rejected. Includes revert and callers using commit_changeset. Metadata trigger prevents invalidating a registered field or source.
- Test changes: request normalization, pending journal, toolbox visibility, Rust registry unit tests, PostGIS integration covering new output geometries, attributes, CRS, relation constraint rejection, tenant isolation and rollback.
- Remaining: real CI/fixture verification, QGIS golden comparison for expanded algorithms, relation form widgets, alternative write-path audit, VN-2000 provenance and all other open SPEC acceptance rows. No live deployment.
