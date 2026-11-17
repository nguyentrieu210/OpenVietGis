# Feature Specification: Spatial Catalog & CRS

**Feature Branch**: `003-spatial-catalog-crs`

**Created**: 2026-11-18

**Status**: Draft — research/specify only; clarification and R002 acceptance required before implementation

**Input**: Parent roadmap: [ROADMAP.md, R003](../../ROADMAP.md#r003). Let authorized users register a spatial source, organize it into a reusable Layer within a Project, discover its schema, geometry and coordinate reference information, and read an explicitly supported reference dataset safely. Prepare the contract used by R004 Map Workspace and R005 Import & Export, without implementing either.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Discover a trusted spatial source (Priority: P1)

A GIS user working inside an authorized Project opens the catalog, registers an available reference source and sees its metadata, supported geometry, attribute schema, feature count/extent state and CRS before using it.

**Why this priority**: The user must know what data is available and what its coordinates mean before mapping or editing it.

**Independent Test**: In an active Project with an authorized pre-established reference source, register it, view the catalog and compare its metadata with the source's known reference facts; no map editor or file importer required.

**Acceptance Scenarios**:

1. **Given** a supported reference source with a known geometry family and CRS, **When** an authorized user registers it in a Project, **Then** the catalog returns a stable source identity, provenance, schema, geometry description, CRS, extent state and readable status.
2. **Given** a reference source has no reliable CRS, **When** registration is requested, **Then** the source cannot be reported ready for coordinate-dependent reads and the user receives an actionable reason; coordinates remain unchanged.
3. **Given** the source is empty, **When** its metadata is requested, **Then** count is zero and extent is explicitly absent rather than a fabricated world extent.
4. **Given** the supplied metadata conflicts with the actual geometry family, dimensions or CRS, **When** validation occurs, **Then** the catalog reports the mismatch and never silently coerces it.

---

### User Story 2 - Create and inspect a Layer (Priority: P1)

A Project user creates a named Layer referencing a supported source and checks its independent identity, schema and available operations. The same source may be represented by multiple Layers within the authorized ownership boundary.

**Why this priority**: A Layer is a logical GIS work resource, not a synonym for a source, file, table, display engine or physical dataset.

**Independent Test**: Register one known source and create two Layers pointing to it; read each Layer and prove both refer to the same source without duplicating the feature data.

**Acceptance Scenarios**:

1. **Given** a ready source in the permitted scope, **When** an authorized user creates a Layer from it, **Then** the Layer has a stable identity, name, source reference, revision and explicit capability information.
2. **Given** a source only supports read operations in R003, **When** its Layer is inspected, **Then** the available read operation is discoverable and unavailable operations such as edit, export and offline use are not falsely advertised.
3. **Given** two Layers reference the same source within the selected ownership boundary, **When** one Layer is retired, **Then** the source and the other Layer remain intact.
4. **Given** a Layer is referencing a ready source, **When** an authorized user lists Layer properties, **Then** source schema, CRS and availability remain distinguishable from Layer-specific metadata.

---

### User Story 3 - Read a spatial reference dataset correctly (Priority: P1)

A GIS user inspects a finite, stable page of records from a Layer, including feature identity and typed attributes; where a supported coordinate read is requested, the returned geometry explicitly identifies its CRS and follows the published coordinate conventions.

**Why this priority**: R004 cannot build truthful map selection and attribute viewing without a dependable, tenant-safe feature read.

**Independent Test**: Retrieve the reference dataset by pages, compare known feature identifiers, attribute values, nulls, geometry and CRS to independently prepared expected results.

**Acceptance Scenarios**:

1. **Given** a readable Layer with more records than the page limit, **When** a user traverses the pages at a fixed source revision, **Then** each expected feature appears once and no unauthorized record appears.
2. **Given** features contain known typed values and nulls, **When** they are read, **Then** the values retain their declared meanings and missing values are distinguishable from empty string and zero.
3. **Given** source and requested read CRS differ and the transformation is supported, **When** the user reads transformed coordinates, **Then** the returned CRS and transformation identity are explicit, control-point tolerance is met and source coordinates are unchanged.
4. **Given** an unsupported or contradictory CRS declaration, a known violation of the declared axis convention, an unavailable transformation or a changed source revision, **When** a read cannot be fulfilled reliably, **Then** the request fails with an actionable reason rather than returning mislabeled or silently inconsistent data.

---

### User Story 4 - Preserve tenant boundaries and trace changes (Priority: P1)

A Project member and manager can use their authorized catalog actions while another Project or Organization cannot discover or change those resources. Changes to catalog registration, Layer metadata, CRS declarations or lifecycle remain attributable and recoverable.

**Why this priority**: Product safety requires tenancy from the first real GIS resource, not as a future SaaS enhancement.

**Independent Test**: Use two Organizations and two Projects with separate grants; verify allowed reads/mutations and denial across direct IDs, listing, references and feature reads, then verify a successful and failed mutation's history.

**Acceptance Scenarios**:

1. **Given** an actor has read but not mutation authority, **When** they read authorized Layer metadata and try to register or retire a Layer, **Then** only the read succeeds.
2. **Given** an actor cannot access another Project or Organization, **When** they request its source, Layer, feature, schema or extent by direct identifier or list, **Then** no protected identity, metadata, count or spatial extent is disclosed.
3. **Given** a user's relevant access is revoked or a Project is no longer writable, **When** the user repeats the affected request, **Then** access is denied according to the governing Project and Organization lifecycle rules.
4. **Given** an authorized actor changes Layer metadata, **When** the change commits, **Then** the actor, prior/current revision, affected resource, time, operation and actual changed values or equivalent safe change evidence are recorded; a failed change leaves no partial resource or revision.

---

### User Story 5 - Inspect Vietnam-first coordinate reference support (Priority: P2)

A GIS professional can identify the exact supported coordinate reference definition rather than choosing a vague label such as “VN-2000”, and see its units, axes, applicability and validated operation.

**Why this priority**: Vietnamese use cases need correct coordinates and reliable transformation evidence, while unsupported local variants must fail transparently.

**Independent Test**: Compare supported definitions and coordinate read results to independently sourced control coordinates, including deliberately swapped axes or wrong CRS examples.

**Acceptance Scenarios**:

1. **Given** two definitions share the VN-2000 datum but differ in projection details, **When** the user views the catalog metadata, **Then** the definitions remain distinguishable and neither is treated as a generic interchangeable VN-2000.
2. **Given** a reference coordinate falls outside the validated transformation area or no accepted transformation exists, **When** the user requests a conversion, **Then** the system reports unsupported/unvalidated rather than silently approximating.
3. **Given** independently sourced control points for a supported transformation, **When** the known coordinate pairs are compared, **Then** the error is within the declared profile-specific tolerance and the method and input evidence can be reproduced.

### Edge Cases

- Source CRS missing, unknown, ambiguous, contradictory or outside supported profile.
- Empty source, invalid geometry, null geometry, null field, empty string, mixed geometry family or unsupported dimensions; never disguise unknown or unavailable metadata as a valid zero value.
- Axis order confusion, meters versus degrees, geographic longitude range and extent wrap; known violations of declared coordinate conventions must be reported explicitly. Plausible values under more than one interpretation cannot reliably reveal mislabeled coordinates without independent evidence.
- Duplicate display names are not identity collisions; a Layer/source reference uses stable identity, not a name.
- A missing/stale/unreadable source or stale revision cannot be silently served as current.
- Project moves between Workspaces may alter inherited authorization; they do not transfer Organization ownership.
- Cross-Project and cross-Organization direct ID substitution, including machine actor or credential with narrower authority.
- Archived or deleted Project, suspended Organization, revoked membership/grant/credential.
- Interrupted registration/retirement, concurrent metadata mutation, stale revision, and attempted destructive removal while references are present.
- Unbounded feature reads or page requests larger than the published limit.

## Requirements *(mandatory)*

### Functional Requirements

#### Scope and catalog semantics

- **FR-001**: Authoritative ownership of each catalog resource MUST be explicit and linked to an Organization and its allowed resource scope.
- **FR-002**: System MUST define the intended source ownership boundary: [NEEDS CLARIFICATION: Are source and Layer project-scoped initially (recommended), or are sources registered at Organization scope and reusable by multiple Projects with distinct sharing authority?]
- **FR-003**: Users with appropriate authority MUST be able to register, list, inspect and retire supported sources within the allowed scope.
- **FR-004**: System MUST distinguish a DataSource, its optional original SourceAsset, a logical Layer and underlying feature data; source registration does not mean automatic import or an editable Layer.
- **FR-005**: Each registered source MUST have a stable identifier, name, type, ownership, provenance, availability/validation state, revision and timestamps.
- **FR-006**: Catalog metadata MUST expose available schema, geometry family/dimensions, feature count state, extent state, source CRS and actual readable coordinate CRS; unknown/unavailable values MUST be explicitly represented.
- **FR-007**: The user MUST be able to register a supported reference source without a full file import workflow. [NEEDS CLARIFICATION: Limit R003 to pre-established, platform-managed reference sources (recommended), or include direct connection to an external spatial database this slice?]
- **FR-008**: System MUST validate source metadata against the reference data before marking it ready; invalid or inconsistent results MUST identify the reason and prevent unreliable operations.
- **FR-009**: Authorized users MUST be able to create, list, inspect, rename and retire logical Layers without duplicating or mutating referenced feature data.
- **FR-010**: System MUST permit more than one logical Layer referencing a single source within the selected allowed ownership boundary; retiring one Layer MUST NOT retire the source or another Layer.
- **FR-011**: A source MUST NOT be physically destroyed by a metadata/Layer retirement operation; retirement that would invalidate active Layers is either rejected with dependency guidance or governed by an explicit, approved later lifecycle workflow.

#### CRS and spatial correctness

- **FR-012**: A source and every coordinate-bearing read response MUST expose an unambiguous CRS identity, coordinate units and declared axis-order convention where applicable. Coordinate-bearing input MUST identify or follow the explicit convention of its declared profile; the system MUST NOT silently guess or swap axes.
- **FR-013**: System MUST distinguish source CRS, stored CRS and response/display CRS, including cases where they coincide; R003 MUST NOT assign a missing CRS silently or overwrite coordinates as a side effect of reading.
- **FR-014**: System MUST publish an explicit, finite supported CRS and transformation matrix: [NEEDS CLARIFICATION: Start with WGS84/Web Mercator plus explicitly chosen and validated VN-2000 profiles (recommended), or permit user-defined CRS parameters in R003 as well?]
- **FR-015**: Supported transformations MUST identify the approved operation, applicability, units/axis order and declared tolerance; unknown or inadequate transformation accuracy MUST NOT be presented as validated.
- **FR-016**: Supported CRS profiles MUST include Vietnam-first coverage, distinguishing datum, projection parameters and applicable region; a generic text label of VN-2000 alone MUST NOT satisfy an exact-CRS requirement.
- **FR-017**: Missing, ambiguous or contradictory source CRS MUST prevent coordinate-dependent ready status until corrected through an explicit, attributable action; unsupported response CRS or transformation MUST fail explicitly. Known inconsistent declared axis, control-point or domain inputs MUST be rejected; plausible numeric values alone MUST NOT be treated as proof of a correctly labeled CRS or axis order.
- **FR-018**: Each supported operation that changes coordinate values MUST have reproducible, independently sourced control-point verification; a round-trip through the same transformation is not sufficient by itself.
- **FR-019**: Geometry metadata MUST distinguish empty, absent, invalid or unsupported geometry without silently changing the original feature.
- **FR-020**: A reported extent MUST carry its own CRS and defined extent semantics; an empty dataset has no extent, not a fabricated bounding box.

#### Capabilities and read access

- **FR-021**: Layers MUST expose explicit available capabilities, independently of source name/type and UI implementation.
- **FR-022**: The initial supported reference Layer MUST expose authorized metadata and basic feature-read capabilities. R003 MUST NOT advertise unsupported rendering, editing, styling, exporting, analysis or offline operations as operational.
- **FR-023**: Capability availability MUST be distinguishable from actor authorization and resource availability; technical ability alone MUST NOT grant access.
- **FR-024**: Authorized basic feature reads MUST return stable feature identity within its source, typed attributes, declared geometry/CRS when requested, source revision and explicit pagination information.
- **FR-025**: Reads MUST be bounded by a documented page limit; traversing pages against an unchanged revision MUST produce deterministic ordering and no missing or duplicate feature identities.
- **FR-026**: If the source revision changes during traversal and consistent paging is not provided, the user MUST receive a stale/retry indication; no silent mixing of inconsistent revisions.
- **FR-027**: System MUST expose enough basic read metadata for R004 to select a feature and inspect attributes without having to know the data provider's physical storage or bypass the catalog.
- **FR-028**: Source credentials, private storage addresses and internal physical paths MUST NOT be exposed in normal catalog or feature-read output.

#### Ownership, permissions, revisions and recovery

- **FR-029**: Every protected action MUST resolve current actor authority and the explicitly selected Organization and Project/resource relationship; authorization MUST enforce the selected resource scope consistently for catalog, detail, schema, extent, capability and feature reads.
- **FR-030**: Cross-Organization references MUST be impossible and access MUST fail without disclosing whether protected identifiers exist; access within the same Organization MUST still obey Project grants.
- **FR-031**: Current R002 lifecycle rules MUST govern reads/mutations of archived/deleted Projects, suspended Organizations and revoked actors; Workspace reassignment MUST NOT change Organization ownership.
- **FR-032**: Human and supported machine actors MUST use the same permission semantics, subject to any credential ceiling and current access; hard-coded role-name shortcuts are prohibited.
- **FR-033**: Material source registration, explicit CRS metadata corrections, schema changes when supported, Layer changes and retirements MUST have attributable, logically immutable change/revision evidence, including actor, time, operation, affected identifiers, prior/current revision information and what actually changed: safe old/new values, a metadata diff or an equivalent retrievable change representation. Change evidence MUST omit secrets and must not disclose protected source data outside its authorized scope.
- **FR-034**: A material mutation and its required revision evidence MUST complete consistently; interrupted or failed actions MUST leave no ready partial resource, orphan reference or successful change record.
- **FR-035**: Competing changes based on a stale resource revision MUST fail with a clear conflict result unless a safe, explicitly specified mechanism preserves both changes.
- **FR-036**: Users MUST receive a useful reason for unsupported, invalid, stale or unauthorized requests without disclosing another tenant's identifiers or protected data.
- **FR-037**: An authorized, durable catalog state MUST survive application restart and an interrupted operation MUST be safely retryable or recoverable through a documented workflow.
- **FR-038**: Source and Layer entities MUST remain independently reusable by later OpenVietGIS capabilities; no domain module, renderer, CAD workflow or file format may define their core identity.

### Key Entities

- **DataSource**: Registered logical source of readable spatial records; owner, source kind, provenance, schema/geometry description, source/stored CRS, revision and lifecycle.
- **SourceAsset (reference only when applicable)**: Original immutable source artifact identity and provenance; not synonymous with DataSource or Layer. File ingestion is deferred to R005.
- **Layer**: Logical work resource, independently identified and named, with source reference, permitted scope and revision; exposes capabilities.
- **CRS Definition/Profile**: Unambiguous definition, authority/code or approved unique identifier, datum, projection parameters where applicable, units, axes and region of applicability.
- **Transformation Profile**: Approved source/target definitions, operation identity, expected accuracy/tolerance, area of use and verification evidence.
- **Feature**: Identifiable source record with attributes and optional geometry, expressed with a declared CRS when coordinates are read.
- **Schema/Extent Summary**: Field names/types/nullability, geometry families/dimensions, counts, extent and explicit unavailable/empty states.
- **Capability**: Named supported operation, whose availability is distinct from current actor authority and resource status.
- **Catalog Change/Revision**: Attributable history of meaningful resource changes, with prior/current revision, safe changed-value evidence and recoverable consistency.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: 100% of the chosen reference sources can be registered and inspected by an authorized user, with schema, count state, CRS and extent matching their independent expected metadata.
- **SC-002**: At least one supported source can be represented by two distinct Layers without duplicating records; retiring either Layer leaves the other Layer and source accessible.
- **SC-003**: For an unchanged reference dataset, paginated reads return 100% of expected feature identities exactly once; known attributes and null semantics match the reference.
- **SC-004**: 100% of control points for each supported transformation meet the independently defined profile tolerance. All agreed negative fixtures with unknown, contradictory or unvalidated CRS declarations or demonstrable violations of the explicit axis convention (control-point mismatch or domain inconsistency) fail with an actionable reason. This criterion does not claim that plausible mislabeled coordinates can be identified from values alone.
- **SC-005**: 100% of the agreed cross-tenant and cross-Project negative checks (list, detail, reference, feature, schema, extent and mutation; human and applicable machine actor) disclose no protected data and mutate no unauthorized resources.
- **SC-006**: 100% of successful material catalog mutations retain expected actor, time, resource, prior/current revision and inspectable safe evidence of the actual changed values; failed or stale operations leave no partial committed catalog state.
- **SC-007**: On the agreed local reference environment, a catalog list/details read has p95 completion at most 1 second and a bounded 100-feature page has p95 completion at most 2 seconds under the proposed initial benchmark dataset (10,000 features across supported geometry families; profile to be finalized before planning). Measure after warm-up, with at least 100 operations and the same dataset/environment recorded.
- **SC-008**: An independent R004 consumer can use catalog metadata and read a chosen feature from the reference data without needing physical storage details or unsupported operations.
- **SC-009**: Restart and simulated interrupted registration checks preserve 100% of previously committed metadata/revisions and leave no ready partial registration.

## Assumptions

- Target user is a GIS-capable person working in an authorized Project, initially on local deployment; no public/anonymous catalog or vertical module.
- Prerequisite R002 is in progress at the base commit; its planned permission registry, actor context, Project lifecycle and tenant enforcement are inputs, not implemented or independently accepted behavior. **R003 implementation and convergence are blocked until R002 is converged and relevant contracts are reconciled.**
- Existing accepted governance: Constitution, ADR-005 (spatial source of truth), ADR-006 (Layer/capabilities), ADR-007 (changes/revisions), ADR-010 (source artifact boundary), ADR-014 (explicit tenant context).
- Reference profile to finalize in clarification/plan: dataset license/provenance, geometry types/dimensions, reference size, exact CRS IDs, independent control coordinates, operation tolerance and test environment. No claim of full nationwide cadastral accuracy until evidence is accepted.
- Catalog metadata and basic feature reading are in scope; full map workspace, style/render engine, feature editor/undo/redo, arbitrary external-source services, multi-format file import/export, spatial query engine, processing jobs, CAD and offline are excluded.
- Basic read consumer is not a promise that R004 rendering is already supported. R003 can be validated through catalog inspection and the documented read contract; no full Map Workspace UI.
- Proposed benchmark size and timings are initial testable targets, not measurements or guarantees. Any changes require an explicit recorded decision before acceptance.
- Retirement in R003 is non-destructive, not physical data purge.
