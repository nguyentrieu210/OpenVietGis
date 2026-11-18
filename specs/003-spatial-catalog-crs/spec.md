# Feature Specification: Spatial Catalog & CRS

**Feature Branch**: `003-spatial-catalog-crs`

**Created**: 2026-11-18

**Status**: Research-prepared draft — first-slice CRS/control/fixture research gates satisfied; R002 convergence and verified-main promotion still required before plan/implementation

**Input**: Parent roadmap: [ROADMAP.md, R003](../../ROADMAP.md#r003). Let authorized users register a spatial source, organize it into a reusable Layer within a Project, discover its schema, geometry and coordinate reference information, and read an explicitly supported reference dataset safely. Prepare the contract used by R004 Map Workspace and R005 Import & Export, without implementing either.

## Clarifications

### Session 2026-11-18

- Q: Is the catalog Project-scoped or reusable across Projects in an Organization? → A: Project-scoped DataSource and Layer under explicit Organization ownership. Multiple Layers within one Project may share its source; cross-Project references and Organization-wide sharing are deferred.
- Q: Which coordinate reference systems are supported in the first slice? → A: A finite explicit matrix consisting of WGS84, Web Mercator and a selected verified VN-2000 profile set. Unknown, unsupported or unvalidated systems and operations are rejected; arbitrary user-defined CRS parameters are deferred. Exact VN-2000 definitions, operations, applicable regions, independently sourced control points and tolerances remain research gates, not decisions made by this clarification.
- Q: Which source registration mode is required before the import feature? → A: Register and read a pre-established platform-managed reference source populated by reproducible fixtures. Full file ingestion is R005; arbitrary database connections or external-source adapters are not part of R003.

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
5. **Given** an arbitrary uploaded file or external database connection is offered for registration, **When** the user tries to use R003 registration, **Then** it is not accepted as a supported source; no file import or external-data credential is persisted.

---

### User Story 2 - Create and inspect a Layer (Priority: P1)

A Project user creates a named Layer referencing a supported source and checks its independent identity, schema and available operations. The same source may be represented by multiple Layers within its owning Project; a different Project cannot reference that registered source.

**Why this priority**: A Layer is a logical GIS work resource, not a synonym for a source, file, table, display engine or physical dataset.

**Independent Test**: Register one known source and create two Layers pointing to it; read each Layer and prove both refer to the same source without duplicating the feature data.

**Acceptance Scenarios**:

1. **Given** a ready source in the same Project, **When** an authorized user creates a Layer from it, **Then** the Layer has a stable identity, name, source reference, revision and explicit capability information.
2. **Given** a source only supports read operations in R003, **When** its Layer is inspected, **Then** the available read operation is discoverable and unavailable operations such as edit, export and offline use are not falsely advertised.
3. **Given** two Layers reference the same source within one Project, **When** one Layer is retired, **Then** the source and the other Layer remain intact.
4. **Given** a Layer is referencing a ready source, **When** an authorized user lists Layer properties, **Then** source schema, CRS and availability remain distinguishable from Layer-specific metadata.
5. **Given** an actor can manage two Projects in the same Organization, **When** they attempt to create a Layer in one Project referencing a registered source in the other, **Then** the reference is rejected and no cross-Project link is created.

---

### User Story 3 - Read a spatial reference dataset correctly (Priority: P1)

A GIS user inspects a finite, stable page of records from a Layer, including feature identity and typed attributes; where a supported coordinate read is requested, the returned geometry explicitly identifies its CRS and follows the published coordinate conventions.

**Why this priority**: R004 cannot build truthful map selection and attribute viewing without a dependable, tenant-safe feature read.

**Independent Test**: Retrieve the reference dataset by pages, compare known feature identifiers, attribute values, nulls, geometry and CRS to independently prepared expected results.

**Acceptance Scenarios**:

1. **Given** a readable Layer with more records than the page limit, **When** a user traverses the pages at a fixed source revision, **Then** each expected feature appears once and no unauthorized record appears.
2. **Given** features contain known typed values and nulls, **When** they are read, **Then** the values retain their declared meanings and missing values are distinguishable from empty string and zero.
3. **Given** source and requested read CRS differ and the transformation is supported, **When** the user reads transformed coordinates, **Then** the returned CRS and transformation identity are explicit, control-point tolerance is met and source coordinates are unchanged.
4. **Given** an unsupported or contradictory CRS declaration, a known violation of the declared axis convention, an unavailable transformation or changed source content/schema revisions, **When** a read cannot be fulfilled reliably, **Then** the request fails with an actionable reason rather than returning mislabeled or silently inconsistent data.
5. **Given** a usable Layer, a returned stable feature identity and the pinned content/schema revisions, **When** the user requests that feature's bounded detail, **Then** its typed attributes and optional geometry agree with the page; requesting an incompatible revision or missing/denied identity cannot substitute another feature.

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
2. **Given** a reference coordinate falls outside the validated transformation area, no accepted transformation exists or the caller supplies arbitrary user-defined CRS parameters, **When** the user requests a conversion, **Then** the system reports unsupported/unvalidated rather than silently approximating.
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

- **FR-001**: Authoritative ownership of each catalog resource MUST be explicit and linked to its Organization and owning Project.
- **FR-002**: DataSource and Layer MUST be Project-scoped with explicit Organization ownership; references across Projects, including Projects in the same Organization, MUST be rejected. Organization-wide reusable source catalogs and separate source-sharing authority are out of R003 scope.
- **FR-003**: Users with appropriate authority MUST be able to register, list, inspect and retire supported sources within the allowed scope.
- **FR-004**: System MUST distinguish a DataSource, its optional original SourceAsset, a logical Layer and underlying feature data; source registration does not mean automatic import or an editable Layer.
- **FR-005**: Each registered source MUST have a stable identifier, name, type, ownership, provenance, availability/validation state, and timestamps. Its catalog metadata/lifecycle revision MUST be distinguishable from a source content revision (the readable feature set, geometry, values and coordinate interpretation) and schema revision. A descriptive metadata-only change that does not affect feature interpretation MUST NOT be silently treated as a new feature set; incompatible schema or content changes MUST invalidate or explicitly resolve affected pinned reads.
- **FR-006**: Catalog metadata MUST expose available schema, geometry family/dimensions, feature count state, extent state, source CRS and actual readable coordinate CRS; unknown/unavailable values MUST be explicitly represented. Each supported source field MUST have a stable logical identifier distinct from its display name and position, plus its type/nullability under an identifiable source schema revision. Reads MUST identify fields unambiguously; renaming a field MUST NOT substitute a different field identity. Schema mutation remains outside this first read slice.
- **FR-007**: The user MUST be able to register and read supported pre-established, platform-managed reference sources populated by reproducible fixtures; registration does not ingest arbitrary uploaded files. Multi-format file import is R005; arbitrary external database connections, credentials and external-source adapters are out of R003 scope.
- **FR-008**: System MUST validate source metadata against the reference data before marking it ready; invalid or inconsistent results MUST identify the reason and prevent unreliable operations.
- **FR-009**: Authorized users MUST be able to create, list, inspect, rename and retire logical Layers without duplicating or mutating referenced feature data. Layer metadata/lifecycle has its own revision, independent of source content and source schema revisions.
- **FR-010**: System MUST permit more than one logical Layer referencing a single source within the same owning Project; retiring one Layer MUST NOT retire the source or another Layer.
- **FR-011**: A source MUST NOT be physically destroyed by a metadata/Layer retirement operation; retirement that would invalidate active Layers is either rejected with dependency guidance or governed by an explicit, approved later lifecycle workflow.

#### CRS and spatial correctness

- **FR-012**: A source and every coordinate-bearing read response MUST expose an unambiguous CRS identity, coordinate units and declared axis-order convention where applicable. Coordinate-bearing input MUST identify or follow the explicit convention of its declared profile; the system MUST NOT silently guess or swap axes.
- **FR-013**: System MUST distinguish source CRS, stored CRS and response/display CRS, including cases where they coincide; R003 MUST NOT assign a missing CRS silently or overwrite coordinates as a side effect of reading.
- **FR-014**: System MUST publish an explicit, finite supported CRS and transformation matrix covering documented WGS84 and Web Mercator definitions plus an explicitly selected and independently validated VN-2000 profile set. Unknown, unsupported or unvalidated definitions or operations MUST be rejected; arbitrary user-defined CRS parameters are out of scope. Exact VN-2000 definitions, applicable regions, transformation operations, independent control points and profile-specific tolerances MUST be finalized at the research gate before plan acceptance, without guessing from a geographic label.
- **FR-015**: Supported transformations MUST identify the approved operation, applicability, units/axis order and declared tolerance; unknown or inadequate transformation accuracy MUST NOT be presented as validated.
- **FR-016**: Supported CRS profiles MUST include Vietnam-first coverage, distinguishing datum, projection parameters and applicable region; a generic text label of VN-2000 alone MUST NOT satisfy an exact-CRS requirement.
- **FR-017**: Missing, ambiguous or contradictory source CRS MUST prevent coordinate-dependent ready status until corrected through an explicit, attributable action; unsupported response CRS or transformation MUST fail explicitly. Known inconsistent declared axis, control-point or domain inputs MUST be rejected; plausible numeric values alone MUST NOT be treated as proof of a correctly labeled CRS or axis order.
- **FR-018**: Each supported operation that changes coordinate values MUST have reproducible, independently sourced control-point verification; a round-trip through the same transformation is not sufficient by itself.
- **FR-019**: Geometry metadata MUST distinguish empty, absent, invalid or unsupported geometry without silently changing the original feature.
- **FR-020**: A reported extent MUST carry its own CRS and defined extent semantics; an empty dataset has no extent, not a fabricated bounding box.

#### Capabilities and read access

- **FR-021**: Layers MUST expose explicit available capabilities, independently of source name/type and UI implementation.
- **FR-022**: The initial supported reference Layer MUST expose authorized metadata and basic feature-read capabilities. R003 MUST NOT advertise unsupported rendering, editing, styling, exporting, analysis or offline operations as operational.
- **FR-023**: Named technical capability, resource lifecycle/availability, and current actor authorization MUST be distinguishable. Technical ability alone MUST NOT grant access. The accepted permission action required for each registered/read/mutation operation MUST be explicit and reused across entry points, without introducing role-name shortcuts.
- **FR-024**: Authorized basic feature reads MUST return stable feature identity within its source, typed attributes keyed by stable field identity, declared geometry/CRS when requested, explicit source content and schema revision identities, and pagination information where applicable. A response MUST unambiguously identify the owning Organization, Project, Layer and source context without exposing other inaccessible resources.
- **FR-025**: Paged reads MUST be bounded by a documented page limit; traversing pages pinned to unchanged source content and schema revisions MUST produce deterministic ordering and no missing or duplicate feature identities.
- **FR-026**: If the pinned source content or schema revision changes during traversal and consistent paging is not provided, the user MUST receive a stale/retry indication; no silent mixing of incompatible revisions or substitution of field/feature identity. A Layer lifecycle or authority change MUST be re-evaluated even when the data revision still matches.
- **FR-027**: The basic read contract MUST support an authorized bounded detail read of one chosen feature by its stable source feature identity within the current Layer context and identified content/schema revisions, with typed attributes and declared geometry/CRS as supported. Missing, unavailable, denied or stale reads MUST be explicit without revealing inaccessible resources or requiring physical storage details. R004 may extend this contract with a bounded viewport-aware read for rendering, but arbitrary spatial queries, renderer-specific access and unbounded data loading are outside R003.
- **FR-028**: Source credentials, private storage addresses and internal physical paths MUST NOT be exposed in normal catalog or feature-read output.

#### Ownership, permissions, revisions and recovery

- **FR-029**: Every protected action MUST resolve current actor authority and the explicitly selected Organization and Project/resource relationship; authorization MUST enforce the selected resource scope consistently for catalog, detail, schema, extent, capability and feature reads.
- **FR-030**: Cross-Organization references MUST be impossible and access MUST fail without disclosing whether protected identifiers exist; access within the same Organization MUST still obey Project grants; a Layer cannot reference a DataSource from another Project even if the actor has authority in both.
- **FR-031**: Current R002 lifecycle rules MUST govern reads/mutations of archived/deleted Projects, suspended Organizations and revoked actors; Workspace reassignment MUST NOT change Organization ownership.
- **FR-032**: Human and supported machine actors MUST use the same permission semantics, subject to any current credential ceiling and access; hard-coded role-name shortcuts are prohibited. The permission-action mapping MUST be reconciled with accepted R002 scoped permissions and lifecycle semantics for catalog reads, registration, changes and retirement before planning; no independent role or permission model is implied.
- **FR-033**: Material source registration, explicit CRS metadata corrections, schema changes when supported, Layer changes and retirements MUST have attributable, logically immutable change/revision evidence, including actor, time, operation, affected identifiers, prior/current revision identity in the affected catalog, Layer, content or schema domain and what actually changed: safe old/new values, a metadata diff or an equivalent retrievable change representation. Change evidence MUST omit secrets and must not disclose protected source data outside its authorized scope.
- **FR-034**: A material mutation and its required revision evidence MUST complete consistently; interrupted or failed actions MUST leave no ready partial resource, orphan reference or successful change record.
- **FR-035**: Competing catalog changes based on a stale catalog or Layer revision MUST fail with a clear conflict result unless a safe, explicitly specified mechanism preserves both changes. The relevant prior and new revision domain MUST be identifiable in each mutation result; a content/schema read pin is not a replacement for the catalog mutation revision.
- **FR-036**: Users MUST receive a useful reason for unsupported, invalid, stale or unauthorized requests without disclosing another tenant's identifiers or protected data.
- **FR-037**: An authorized, durable catalog state MUST survive application restart and an interrupted operation MUST be safely retryable or recoverable through a documented workflow.
- **FR-038**: Source and Layer entities MUST remain independently reusable by later OpenVietGIS capabilities; no domain module, renderer, CAD workflow or file format may define their core identity.

### Key Entities

- **DataSource**: Registered logical source of readable spatial records; explicit Organization and Project owner, source kind, provenance, schema/geometry description, source/stored CRS, catalog metadata/lifecycle revision, separate content and schema revisions, and lifecycle.
- **SourceAsset (reference only when applicable)**: Original immutable source artifact identity and provenance; not synonymous with DataSource or Layer. File ingestion is deferred to R005.
- **Layer**: Logical work resource, independently identified and named, with source reference in the same Project, explicit Organization and Project owner, and revision; exposes capabilities.
- **CRS Definition/Profile**: Unambiguous definition, authority/code or approved unique identifier, datum, projection parameters where applicable, units, axes and region of applicability.
- **Transformation Profile**: Approved source/target definitions, operation identity, expected accuracy/tolerance, area of use and verification evidence.
- **Feature**: Stable source-scoped logical record identity with attributes keyed by stable field identities under a known schema revision, and optional geometry expressed with a declared CRS. Reads identify the content revision; the same identity at another revision is not automatically the same observed data.
- **Schema/Extent Summary**: Stable field identifiers, names/types/nullability and schema revision, geometry families/dimensions, counts, extent and explicit unavailable/empty states.
- **Capability**: Named supported operation, whose availability is distinct from current actor authority and resource status.
- **Catalog Change/Revision**: Attributable history of meaningful resource changes, with prior/current revision, safe changed-value evidence and recoverable consistency.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: 100% of the chosen reference sources can be registered and inspected by an authorized user, with schema, count state, CRS and extent matching their independent expected metadata.
- **SC-002**: At least one supported source can be represented by two distinct Layers without duplicating records; retiring either Layer leaves the other Layer and source accessible.
- **SC-003**: For an unchanged content/schema revision of the reference dataset, paginated reads return 100% of expected feature identities exactly once; a chosen stable-ID detail read agrees with its page, known stable fields, typed attributes and null semantics. In agreed incompatible revision fixtures, all stale read pins are rejected or explicitly reconciled without substituting a different feature or field.
- **SC-004**: 100% of control points for each supported transformation meet the independently defined profile tolerance. All agreed negative fixtures with unknown, contradictory or unvalidated CRS declarations or demonstrable violations of the explicit axis convention (control-point mismatch or domain inconsistency) fail with an actionable reason. This criterion does not claim that plausible mislabeled coordinates can be identified from values alone.
- **SC-005**: 100% of the agreed cross-tenant and cross-Project negative checks (list, detail, reference, feature, schema, extent and mutation; human and applicable machine actor) disclose no protected data and mutate no unauthorized resources; cross-Project source references fail even where the actor is authorized in both Projects.
- **SC-006**: 100% of successful material catalog mutations retain expected actor, time, resource, prior/current revision and inspectable safe evidence of the actual changed values; failed or stale operations leave no partial committed catalog state.
- **SC-007**: On the agreed local reference environment, a catalog list/details read has p95 completion at most 1 second and a bounded 100-feature page has p95 completion at most 2 seconds under the proposed initial benchmark dataset (10,000 features across supported geometry families; profile to be finalized before planning). Measure after warm-up, with at least 100 operations and the same dataset/environment recorded.
- **SC-008**: An independent contract-test consumer can use catalog metadata, bounded pages and stable-ID detail to read a chosen reference feature without needing physical storage details or unsupported operations, providing evidence usable by future R004. R003 acceptance does not require R004 runtime or implementation.
- **SC-009**: Restart and simulated interrupted registration checks preserve 100% of previously committed metadata/revisions and leave no ready partial registration.

## Assumptions

- Target user is a GIS-capable person working in an authorized Project, initially on local deployment; no public/anonymous catalog or vertical module.
- Prerequisite R002 was incomplete at this historical branch base. Its current authoritative candidate now implements the permission registry, actor context, Project lifecycle and tenant enforcement and has been reconciled in R003 research, but it is not yet converged/promoted because the SC-013 unfamiliar-human acceptance gate remains open. **R003 plan/implementation and convergence remain blocked until R002 is converged and promoted to verified main.** The integrating owner must transplant only the feature's specification documents onto that verified main baseline; this historical branch is not eligible for wholesale merge.
- Existing accepted governance: Constitution, ADR-005 (spatial source of truth), ADR-006 (Layer/capabilities), ADR-007 (changes/revisions), ADR-010 (source artifact boundary), ADR-014 (explicit tenant context).
- The first-slice research profile is now recorded in `crs-support-matrix.md`, `vn2000-control-evidence.md` and `reference-fixture-profile.md`: deterministic 10,000-feature 2D Point/LineString/Polygon fixtures, exact CRS identities, independent Quang Ninh VN-2000 controls, profile tolerances and benchmark environment. This is software acceptance evidence, not a claim of nationwide cadastral/survey accuracy; arbitrary user-defined CRS parameters and unvalidated VN-2000 transformation variants remain unsupported.
- Catalog registration of pre-established platform-managed fixture sources and basic feature reading are in scope; full map workspace, style/render engine, feature editor/undo/redo, arbitrary external database connections and external-source services, multi-format file import/export, spatial query engine, processing jobs, CAD and offline are excluded.
- Basic read consumer is not a promise that R004 rendering is already supported. R003 can be validated through catalog inspection and the documented read contract; no full Map Workspace UI.
- Proposed benchmark size and timings are initial testable targets, not measurements or guarantees. Any changes require an explicit recorded decision before acceptance.
- Retirement in R003 is non-destructive, not physical data purge.
