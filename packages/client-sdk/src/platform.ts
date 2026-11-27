import { ApiClient, projectApiPath } from "./http";

export interface ChangesetOperation {
  id: string;
  ordinal: number;
  op_type: "create" | "update" | "delete";
  feature_id: string;
  before_geometry: Record<string, unknown> | null;
  after_geometry: Record<string, unknown> | null;
  before_properties: Record<string, unknown> | null;
  after_properties: Record<string, unknown> | null;
  created_at: string;
}

export interface Changeset {
  id: string;
  source_id: string;
  actor_display_snapshot: string;
  actor_id?: string;
  operation_id: string;
  base_content_revision: string;
  resulting_content_revision: string;
  base_catalog_revision: number;
  resulting_catalog_revision: number;
  summary: string;
  status: string;
  operation_count: number;
  reverted_from_changeset_id: string | null;
  created_at: string;
  operations: ChangesetOperation[];
}

export interface QueryResultItem {
  feature_id: string;
  attributes: Record<string, unknown>;
  geometry: Record<string, unknown> | null;
}

export interface LayerQueryResponse {
  layer_id: string;
  source_id: string;
  content_revision: string;
  stored_crs: number;
  /** Required by revision-pinned query servers; optional for older servers. */
  schema_revision?: string;
  query_fingerprint?: string;
  reference_pin?: { layer_id: string; source_id: string; content_revision: string; schema_revision: string; stored_crs: number } | null;
  geometry_transformation?: string | null;
  total_count: number;
  limit: number;
  offset: number;
  has_more: boolean;
  selection_feature_ids: string[];
  items: QueryResultItem[];
}

export interface SavedQuery {
  id: string;
  layer_id: string;
  name: string;
  description: string;
  attribute_filter: Record<string, unknown> | null;
  spatial_filter: Record<string, unknown> | null;
  sort_field: string | null;
  sort_direction: string;
  created_at: string;
}

export interface ProcessingAlgorithm {
  id: "buffer" | "clip" | "dissolve";
  name: string;
  description: string;
  required_inputs: string[];
  parameters_schema: Record<string, unknown>;
}

export interface ProcessingJob {
  id: string;
  algorithm_id: "buffer" | "clip" | "dissolve";
  operation_id: string;
  status: "queued" | "running" | "succeeded" | "failed" | "cancelled";
  output_name: string;
  output_crs: number;
  engine_version: string;
  attempt_count: number;
  output_source_id: string | null;
  output_layer_id: string | null;
  feature_count: number | null;
  created_at: string;
  completed_at: string | null;
  error_code?: string | null;
  error_message?: string | null;
  input_refs?: Record<string, unknown>;
  parameters?: Record<string, unknown>;
}

export interface LayerStyle {
  id: string;
  layer_id: string;
  mode: "single" | "categorical" | "range";
  style_spec: Record<string, unknown>;
  label_field: string | null;
  legend_items: Array<{ label: string; fill_color: string; stroke_color: string }>;
  revision: number;
  updated_at: string;
}

export interface PrintLayout {
  id: string;
  name: string;
  title: string;
  paper_size: "A0" | "A1" | "A2" | "A3" | "A4";
  orientation: "portrait" | "landscape";
  crs_code: number;
  attribution: string;
  layer_ids: string[];
  created_at: string;
}

export interface PrintRevisionManifest {
  layout_id: string;
  layout_revision: string;
  crs_code: number;
  output_format: "png" | "pdf";
  dpi: 150 | 300;
  pixel_width: number;
  pixel_height: number;
  scale_denominator: number;
  scale_accuracy_note: string;
  effective_extent: { min_x: number; min_y: number; max_x: number; max_y: number };
  layers: Array<{ layer_id: string; source_id: string; layer_revision: number; content_revision: string; schema_revision: string; style_revision: number }>;
}

export interface PrintExportPreview {
  revision_manifest: PrintRevisionManifest;
  revision_fingerprint: string;
}

export interface PrintExport {
  id: string;
  layout_id: string;
  output_format: "png" | "pdf";
  dpi: number;
  scale_denominator: number;
  scale_accuracy_note: string;
  revision_manifest: Record<string, unknown>;
  sha256: string;
  byte_size: number;
  created_at: string;
}

export interface AllowlistedHost {
  id: string;
  hostname: string;
  allowed_schemes: string[];
  allowed_ports: number[];
  has_secret_ref: boolean;
  secret_ref_name: string | null;
  enabled: boolean;
}

export interface ExternalConnection {
  id: string;
  name: string;
  protocol: "wms" | "wmts_xyz" | "cog_geotiff" | "wfs_ogc_features";
  endpoint_url: string;
  attribution: string;
  license_text: string;
  crs_codes: number[];
  capabilities: Record<string, boolean>;
  read_only: boolean;
  cache_generation: number;
  lifecycle: string;
  availability?: "unverified" | "revoked";
}

export interface ProjectActivity {
  id: string;
  actor_type: string;
  actor_id: string;
  actor_display: string;
  event_kind: string;
  resource_type: string;
  resource_id: string;
  summary: string;
  revision_ref: string | null;
  metadata: Record<string, unknown>;
  created_at: string;
}

export interface ProjectActivityPage {
  items: ProjectActivity[];
  cutoff: string;
  next_cursor: string | null;
}

export interface ChangeReview {
  id: string;
  source_id: string;
  changeset_id: string;
  content_revision: string;
  assignee_user_id: string;
  status: "pending" | "approved" | "changes_requested";
  decision_note: string;
  created_at: string;
}

export interface CadConversion {
  id: string;
  direction: string;
  cad_format: string;
  filename: string;
  units: string;
  crs_code: number;
  status: string;
  entity_summary: Record<string, unknown>;
  loss_report: Record<string, unknown>;
  layer_id: string | null;
  created_at: string;
}

export interface OfflinePackage {
  id: string;
  name: string;
  profile: string;
  crs_code: number;
  feature_count: number;
  sha256: string;
  byte_size: number;
  expires_at: string;
  revoked_at: string | null;
  layer_pins: Array<Record<string, unknown>>;
}

export interface OfficialExtensionCatalogItem {
  extension_key: string;
  version: string;
  display_name: string;
  description: string;
  capabilities: string[];
  ui_contributions: Array<Record<string, unknown>>;
}

export interface ExtensionManifest {
  id: string;
  extension_key: string;
  version: string;
  display_name: string;
  description: string;
  api_compatibility: string;
  enabled: boolean;
  permissions: string[];
  commands: Array<{ id: string; label: string; required_permission: string }>;
  capabilities: string[];
  entity_schemas: Record<string, { fields: Record<string, { type: string; required?: boolean }> }>;
  ui_contributions?: Array<Record<string, unknown>>;
}

export type ExtensionPayload = Record<string, string | number | boolean | null>;
export type ExtensionLinkContext = {
  status: "pinned";
  source_id: string;
  content_revision: string;
  schema_revision: string;
  stored_crs: number;
};
export interface ExtensionRecord {
  id: string;
  extension_key: string;
  entity_name: string;
  manifest_id: string;
  revision: number;
  payload: ExtensionPayload;
  linked_layer_id: string | null;
  linked_feature_id: string | null;
  link_context: ExtensionLinkContext | { status: "legacy_unpinned" } | null;
  created_at: string;
  updated_at: string;
}
export type ExtensionDataRecord = ExtensionRecord;

export interface ExtensionActivation {
  manifest_version?: string | null;
  runtime_compatible?: boolean;
  id: string;
  manifest_id: string;
  extension_key: string;
  status: "active" | "disabled";
  revision?: number;
  activated_at: string;
  disabled_at?: string | null;
  updated_at: string;
}
export interface ExtensionRevision {
  revision: number;
  manifest_id: string;
  entity_name: string;
  payload: ExtensionPayload;
  linked_layer_id: string | null;
  linked_feature_id: string | null;
  link_context: ExtensionRecord["link_context"];
  actor_id: string;
  occurred_at: string;
  event: "legacy_baseline" | "create" | "update" | "import";
  import_origin: Record<string, unknown> | null;
}
export interface ExtensionLifecycleEvent {
  id: string;
  event: "legacy_baseline" | "activate" | "deactivate" | "transition";
  previous_manifest_id: string | null;
  manifest_id: string;
  status: ExtensionActivation["status"];
  actor_id: string;
  occurred_at: string;
}
export interface ExtensionPortableBundle {
  bundle: {
    format: "openvietgis.extension.portable.v1";
    extension_key: string;
    origin: { organization_id: string; project_id: string };
    active_manifest_id: string;
    activation_status: ExtensionActivation["status"];
    manifests: Array<Omit<ExtensionManifest, "id" | "enabled"> & { manifest_id: string }>;
    activation_history: ExtensionLifecycleEvent[];
    records: Array<Omit<ExtensionRecord, "id" | "extension_key" | "created_at" | "updated_at"> & {
      record_id: string;
      history: ExtensionRevision[];
    }>;
  };
  sha256: string;
}
export interface ExtensionImportReceipt {
  id: string;
  sha256: string;
  mappings: Array<{ source_record_id: string; record_id: string }>;
  replayed: boolean;
}

export interface ReleaseCapability {
  id: string;
  name: string;
  /** Release certification, not a runtime feature switch. */
  enabled: boolean;
  milestone: string;
  status: "unverified" | "unsupported" | "disabled" | "accepted";
  reason: string;
  required_for_team: boolean;
}

export interface ReleaseCapabilityReport {
  profile: string;
  profile_status: "unverified" | "accepted";
  verified_candidate: string | null;
  verification_record: string | null;
  production_ready: boolean;
  enabled_semantics: "release_certification";
  total_releases: number;
  accepted_release_count: number;
  items: ReleaseCapability[];
}

export interface OpsHealthReadiness {
  status: "ready" | "not_ready";
  database: "ok" | "unavailable";
  postgis: "ok" | "unavailable";
  postgis_version: string | null;
  postgresql_version: string | null;
  artifact_store: "ok" | "unavailable";
  scope: "dependency_probe_only";
  production_ready: boolean;
  checked_at: string;
}

function projBase(orgId: string, projectId: string): string {
  return projectApiPath(orgId, projectId);
}

function csrfHeaders(csrf: string): Record<string, string> {
  return { "x-openvietgis-csrf": csrf };
}

// R006 Editing
export function commitChangeset(
  client: ApiClient,
  orgId: string,
  projectId: string,
  sourceId: string,
  body: {
    base_content_revision: string;
    base_schema_revision?: string;
    operation_id: string;
    summary: string;
    operations: Array<{
      op_type: "create" | "update" | "delete";
      feature_id: string;
      geometry?: Record<string, unknown> | null;
      properties?: Record<string, unknown> | null;
    }>;
  },
  csrf: string
): Promise<Changeset> {
  return client.request<Changeset>(
    `${projBase(orgId, projectId)}/sources/${encodeURIComponent(sourceId)}/changesets`,
    { method: "POST", body: JSON.stringify(body), headers: csrfHeaders(csrf) }
  );
}

export function listChangesets(
  client: ApiClient,
  orgId: string,
  projectId: string,
  sourceId: string
): Promise<{ items: Changeset[] }> {
  return client.request<{ items: Changeset[] }>(
    `${projBase(orgId, projectId)}/sources/${encodeURIComponent(sourceId)}/changesets`
  );
}

export function revertChangeset(
  client: ApiClient,
  orgId: string,
  projectId: string,
  changesetId: string,
  operationId: string,
  csrf: string,
  summary?: string
): Promise<Changeset> {
  return client.request<Changeset>(
    `${projBase(orgId, projectId)}/changesets/${encodeURIComponent(changesetId)}/revert`,
    { method: "POST", body: JSON.stringify({ operation_id: operationId, ...(summary ? { summary } : {}) }), headers: csrfHeaders(csrf) }
  );
}

// R007 Queries
export function executeLayerQuery(
  client: ApiClient,
  orgId: string,
  projectId: string,
  layerId: string,
  body: Record<string, unknown>,
  csrf: string
): Promise<LayerQueryResponse> {
  return client.request<LayerQueryResponse>(
    `${projBase(orgId, projectId)}/layers/${encodeURIComponent(layerId)}/query`,
    { method: "POST", body: JSON.stringify(body), headers: csrfHeaders(csrf) }
  );
}

export function listSavedQueries(
  client: ApiClient,
  orgId: string,
  projectId: string
): Promise<{ items: SavedQuery[] }> {
  return client.request<{ items: SavedQuery[] }>(`${projBase(orgId, projectId)}/saved-queries`);
}

export function createSavedQuery(
  client: ApiClient,
  orgId: string,
  projectId: string,
  body: Record<string, unknown>,
  csrf: string
): Promise<SavedQuery> {
  return client.request<SavedQuery>(`${projBase(orgId, projectId)}/saved-queries`, {
    method: "POST",
    body: JSON.stringify(body),
    headers: csrfHeaders(csrf)
  });
}

// R008 Processing
export function listProcessingAlgorithms(
  client: ApiClient,
  orgId: string,
  projectId: string
): Promise<{ items: ProcessingAlgorithm[] }> {
  return client.request<{ items: ProcessingAlgorithm[] }>(
    `${projBase(orgId, projectId)}/processing/algorithms`
  );
}

export function createProcessingJob(
  client: ApiClient,
  orgId: string,
  projectId: string,
  body: Record<string, unknown>,
  csrf: string
): Promise<ProcessingJob> {
  return client.request<ProcessingJob>(`${projBase(orgId, projectId)}/processing/jobs`, {
    method: "POST",
    body: JSON.stringify(body),
    headers: csrfHeaders(csrf)
  });
}

export function listProcessingJobs(
  client: ApiClient,
  orgId: string,
  projectId: string
): Promise<{ items: ProcessingJob[] }> {
  return client.request<{ items: ProcessingJob[] }>(`${projBase(orgId, projectId)}/processing/jobs`);
}

export function cancelProcessingJob(
  client: ApiClient,
  orgId: string,
  projectId: string,
  jobId: string,
  csrf: string
): Promise<ProcessingJob> {
  return client.request<ProcessingJob>(
    `${projBase(orgId, projectId)}/processing/jobs/${encodeURIComponent(jobId)}/cancel`,
    { method: "POST", headers: csrfHeaders(csrf) }
  );
}

export function retryProcessingJob(
  client: ApiClient,
  orgId: string,
  projectId: string,
  jobId: string,
  csrf: string
): Promise<ProcessingJob> {
  return client.request<ProcessingJob>(
    `${projBase(orgId, projectId)}/processing/jobs/${encodeURIComponent(jobId)}/retry`,
    { method: "POST", headers: csrfHeaders(csrf) }
  );
}

// R009 Cartography
export function upsertLayerStyle(
  client: ApiClient,
  orgId: string,
  projectId: string,
  layerId: string,
  body: Record<string, unknown>,
  csrf: string
): Promise<LayerStyle> {
  return client.request<LayerStyle>(
    `${projBase(orgId, projectId)}/cartography/layers/${encodeURIComponent(layerId)}/style`,
    { method: "PUT", body: JSON.stringify(body), headers: csrfHeaders(csrf) }
  );
}

export function listPrintLayouts(
  client: ApiClient,
  orgId: string,
  projectId: string
): Promise<{ items: PrintLayout[] }> {
  return client.request<{ items: PrintLayout[] }>(
    `${projBase(orgId, projectId)}/cartography/layouts`
  );
}

export function createPrintLayout(
  client: ApiClient,
  orgId: string,
  projectId: string,
  body: Record<string, unknown>,
  csrf: string
): Promise<PrintLayout> {
  return client.request<PrintLayout>(`${projBase(orgId, projectId)}/cartography/layouts`, {
    method: "POST",
    body: JSON.stringify(body),
    headers: csrfHeaders(csrf)
  });
}

export function previewPrintExport(
  client: ApiClient,
  orgId: string,
  projectId: string,
  layoutId: string,
  body: { output_format: "png" | "pdf"; dpi: 150 | 300 },
  csrf: string
): Promise<PrintExportPreview> {
  return client.request<PrintExportPreview>(
    `${projBase(orgId, projectId)}/cartography/layouts/${encodeURIComponent(layoutId)}/exports/preview`,
    { method: "POST", body: JSON.stringify(body), headers: csrfHeaders(csrf) }
  );
}

export function exportPrintLayout(
  client: ApiClient,
  orgId: string,
  projectId: string,
  layoutId: string,
  body: { output_format: "png" | "pdf"; dpi: 150 | 300; expected_revision_fingerprint: string },
  csrf: string
): Promise<PrintExport> {
  return client.request<PrintExport>(
    `${projBase(orgId, projectId)}/cartography/layouts/${encodeURIComponent(layoutId)}/exports`,
    { method: "POST", body: JSON.stringify(body), headers: csrfHeaders(csrf) }
  );
}

export function listPrintExports(
  client: ApiClient,
  orgId: string,
  projectId: string
): Promise<{ items: PrintExport[] }> {
  return client.request<{ items: PrintExport[] }>(
    `${projBase(orgId, projectId)}/cartography/exports`
  );
}

export function printExportDownloadUrl(
  orgId: string,
  projectId: string,
  exportId: string
): string {
  return `/api/v1${projBase(orgId, projectId)}/cartography/exports/${encodeURIComponent(exportId)}/download`;
}

// R010 External Sources
export function listAllowlistedHosts(
  client: ApiClient,
  orgId: string
): Promise<{ items: AllowlistedHost[] }> {
  return client.request<{ items: AllowlistedHost[] }>(
    `/organizations/${encodeURIComponent(orgId)}/external-sources/allowlist`
  );
}

export function registerAllowlistedHost(
  client: ApiClient,
  orgId: string,
  body: Record<string, unknown>,
  csrf: string
): Promise<AllowlistedHost> {
  return client.request<AllowlistedHost>(
    `/organizations/${encodeURIComponent(orgId)}/external-sources/allowlist`,
    { method: "POST", body: JSON.stringify(body), headers: csrfHeaders(csrf) }
  );
}

export function listExternalConnections(
  client: ApiClient,
  orgId: string,
  projectId: string
): Promise<{ items: ExternalConnection[] }> {
  return client.request<{ items: ExternalConnection[] }>(
    `${projBase(orgId, projectId)}/external-sources`
  );
}

export function createExternalConnection(
  client: ApiClient,
  orgId: string,
  projectId: string,
  body: Record<string, unknown>,
  csrf: string
): Promise<ExternalConnection> {
  return client.request<ExternalConnection>(`${projBase(orgId, projectId)}/external-sources`, {
    method: "POST",
    body: JSON.stringify(body),
    headers: csrfHeaders(csrf)
  });
}

export interface ExternalProviderPreview {
  availability: "available";
  capabilities: Record<string, boolean>;
  scope: "operator_approved_sample" | "operator_approved_viewport";
  connection_id: string;
  cache_generation: number;
  layer_identifier: string;
  crs: number;
  bbox: [number, number, number, number];
  attribution: string;
  license_text: string;
  image_png_base64?: string;
  width?: number;
  height?: number;
  sample?:
    | { kind: "xyz" | "wmts"; z: number; x: number; y: number }
    | {
        kind: "geotiff_range_sample";
        tile_width: number;
        tile_height: number;
        tile_count: number;
        pixel_size: [number, number];
        band_count: number;
        range_bytes: number;
      }
    | {
        kind: "cog_window_sample";
        tile_width: number;
        tile_height: number;
        tile_count: number;
        band_count: number;
        overview_index: number;
        overview_scale: number;
        overview_count: number;
        source_size: [number, number];
        overview_pixel_window: [number, number, number, number];
        source_pixel_window: [number, number, number, number];
        pixel_size: [number, number];
        compression: "none" | "deflate";
        predictor: number;
        range_requests: number;
        range_bytes: number;
        object_bytes: number;
        allow_viewport: boolean;
        approved_bbox: [number, number, number, number];
      };
  feature_result?: {
    returned: number;
    provider_reported_matched: number | null;
    request_limit: number;
    limit_reached: boolean;
    coverage: "unknown" | "partial" | "provider_reported_complete";
    continuation_available: false;
  };
  feature_collection?: {
    type: "FeatureCollection";
    features: Array<{
      type: "Feature";
      id?: string | number;
      geometry: { type: string; coordinates: unknown };
      properties: Record<string, string | number | boolean | null>;
    }>;
  };
}

export function previewExternalProvider(
  client: ApiClient,
  orgId: string,
  projectId: string,
  connectionId: string,
  csrf: string
): Promise<ExternalProviderPreview> {
  return client.request<ExternalProviderPreview>(
    `${projBase(orgId, projectId)}/external-sources/${encodeURIComponent(connectionId)}/preview`,
    { method: "POST", headers: csrfHeaders(csrf), cache: "no-store" }
  );
}

export interface ExternalViewportRequest {
  crs: number;
  bbox: [number, number, number, number];
  width: number;
  height: number;
}

export function previewExternalProviderViewport(
  client: ApiClient,
  orgId: string,
  projectId: string,
  connectionId: string,
  body: ExternalViewportRequest,
  csrf: string
): Promise<ExternalProviderPreview> {
  return client.request<ExternalProviderPreview>(
    `${projBase(orgId, projectId)}/external-sources/${encodeURIComponent(connectionId)}/viewport`,
    { method: "POST", headers: csrfHeaders(csrf), body: JSON.stringify(body), cache: "no-store" }
  );
}

// R011 Collaboration
export function listProjectActivities(
  client: ApiClient,
  orgId: string,
  projectId: string,
  options: { limit?: number; cursor?: string } = {}
): Promise<ProjectActivityPage> {
  const params = new URLSearchParams();
  if (options.limit !== undefined) params.set("limit", String(options.limit));
  if (options.cursor !== undefined) params.set("cursor", options.cursor);
  const query = params.toString();
  return client.request<ProjectActivityPage>(
    `${projBase(orgId, projectId)}/collaboration/activities${query ? "?" + query : ""}`,
    { cache: "no-store" }
  );
}

export function listChangeReviews(
  client: ApiClient,
  orgId: string,
  projectId: string
): Promise<{ items: ChangeReview[] }> {
  return client.request<{ items: ChangeReview[] }>(
    `${projBase(orgId, projectId)}/collaboration/reviews`
  );
}

export interface ReviewComment {
  id: string;
  review_id: string;
  changeset_id: string;
  content_revision: string;
  feature_id: string | null;
  body: string;
  author_actor_id: string;
  author_display: string;
  created_at: string;
}

export function getChangeset(
  client: ApiClient, orgId: string, projectId: string, changesetId: string
): Promise<Changeset> {
  return client.request<Changeset>(`${projBase(orgId, projectId)}/changesets/${encodeURIComponent(changesetId)}`);
}

export function createChangeReview(
  client: ApiClient, orgId: string, projectId: string,
  body: { changeset_id: string; assignee_user_id: string }, csrf: string
): Promise<ChangeReview> {
  return client.request<ChangeReview>(`${projBase(orgId, projectId)}/collaboration/reviews`, {
    method: "POST", body: JSON.stringify(body), headers: csrfHeaders(csrf)
  });
}

export function listReviewComments(
  client: ApiClient, orgId: string, projectId: string, reviewId: string
): Promise<{ items: ReviewComment[] }> {
  return client.request<{ items: ReviewComment[] }>(
    `${projBase(orgId, projectId)}/collaboration/reviews/${encodeURIComponent(reviewId)}/comments`
  );
}

export function addReviewComment(
  client: ApiClient, orgId: string, projectId: string, reviewId: string,
  body: { body: string; feature_id: string | null; operation_id?: string }, csrf: string
): Promise<ReviewComment> {
  return client.request<ReviewComment>(
    `${projBase(orgId, projectId)}/collaboration/reviews/${encodeURIComponent(reviewId)}/comments`,
    { method: "POST", body: JSON.stringify(body), headers: csrfHeaders(csrf) }
  );
}

export function decideChangeReview(
  client: ApiClient,
  orgId: string,
  projectId: string,
  reviewId: string,
  body: { status: "approved" | "changes_requested"; decision_note: string },
  csrf: string
): Promise<ChangeReview> {
  return client.request<ChangeReview>(
    `${projBase(orgId, projectId)}/collaboration/reviews/${encodeURIComponent(reviewId)}/decide`,
    { method: "POST", body: JSON.stringify(body), headers: csrfHeaders(csrf) }
  );
}

// R012 CAD & COGO
export interface CadDxfInput {
  file: File;
  units: "meter" | "centimeter" | "millimeter";
  crsCode: 3857 | 5899;
  curveToleranceM?: number;
}

export interface CadDxfInspection {
  sha256: string;
  inspection_fingerprint: string;
  published: false;
  geometry_valid: boolean;
  entity_summary: {
    entity_counts: Record<string, number>;
    layer_counts: Record<string, number>;
    published_family: string;
    published_feature_count: number;
    declared_insunits: number;
    crs_code: number;
    spatial_mapping: string;
  };
  loss_report: {
    unsupported_entities: Record<string, number>;
    dropped_secondary_families: Record<string, number>;
    curve_approximations: string[];
    curve_tolerance_grid_m: number | null;
    curve_details: Array<{ handle: string; entity: string; max_chord_deviation_grid_m: number }>;
    requires_acknowledgement: boolean;
    survey_grade: false;
  };
}

function cadForm(input: CadDxfInput): FormData {
  const form = new FormData();
  form.append("file", input.file);
  form.append("units", input.units);
  form.append("crs_code", String(input.crsCode));
  if (input.curveToleranceM !== undefined) form.append("curve_tolerance_m", String(input.curveToleranceM));
  return form;
}

export function inspectDxf(
  client: ApiClient, orgId: string, projectId: string, input: CadDxfInput, csrf: string
): Promise<CadDxfInspection> {
  return client.request<CadDxfInspection>(`${projBase(orgId, projectId)}/cad/inspect`, {
    method: "POST", body: cadForm(input), headers: csrfHeaders(csrf)
  });
}

export function importInspectedDxf(
  client: ApiClient, orgId: string, projectId: string,
  input: CadDxfInput, inspection: CadDxfInspection, layerName: string, acknowledgeLoss: boolean, csrf: string
): Promise<CadConversion> {
  const form = cadForm(input);
  form.append("layer_name", layerName);
  form.append("expected_sha256", inspection.sha256);
  form.append("expected_inspection_fingerprint", inspection.inspection_fingerprint);
  form.append("acknowledge_loss", String(acknowledgeLoss));
  return client.request<CadConversion>(`${projBase(orgId, projectId)}/cad/import`, {
    method: "POST", body: form, headers: csrfHeaders(csrf)
  });
}

export function listCadConversions(
  client: ApiClient,
  orgId: string,
  projectId: string
): Promise<{ items: CadConversion[] }> {
  return client.request<{ items: CadConversion[] }>(
    `${projBase(orgId, projectId)}/cad/conversions`
  );
}

export interface PlanarTraversePreview {
  endpoint: [number, number];
  delta: [number, number];
  coordinate_reference: "unreferenced_planar_input";
  units: "input_coordinate_units";
  survey_grade: false;
  published: false;
}

export function cogoTraverse(
  client: ApiClient,
  orgId: string,
  projectId: string,
  body: { origin: [number, number]; distance: number; azimuth_degrees: number },
  csrf: string
): Promise<PlanarTraversePreview> {
  return client.request<PlanarTraversePreview>(
    `${projBase(orgId, projectId)}/cad/precision/traverse`,
    { method: "POST", body: JSON.stringify(body), headers: csrfHeaders(csrf) }
  );
}

// R013 Field & Offline
export function listOfflinePackages(
  client: ApiClient,
  orgId: string,
  projectId: string
): Promise<{ items: OfflinePackage[] }> {
  return client.request<{ items: OfflinePackage[] }>(
    `${projBase(orgId, projectId)}/field/packages`
  );
}

export function createOfflinePackage(
  client: ApiClient,
  orgId: string,
  projectId: string,
  body: Record<string, unknown>,
  csrf: string
): Promise<OfflinePackage> {
  return client.request<OfflinePackage>(`${projBase(orgId, projectId)}/field/packages`, {
    method: "POST",
    body: JSON.stringify(body),
    headers: csrfHeaders(csrf)
  });
}

export function offlinePackageDownloadUrl(
  orgId: string,
  projectId: string,
  packageId: string
): string {
  return `/api/v1${projBase(orgId, projectId)}/field/packages/${encodeURIComponent(packageId)}/download`;
}

export interface OfflineOperationReceipt {
  client_operation_id: string;
  local_feature_id: string;
  core_feature_id: string | null;
  status: "applied" | "not_applied";
  changeset_id: string | null;
}

export function syncOfflineOutbox(
  client: ApiClient,
  orgId: string,
  projectId: string,
  packageId: string,
  body: { operation_id: string; operations: Array<Record<string, unknown>> },
  csrf: string
): Promise<{ id: string; status: string; applied_changeset_ids: string[]; conflicts: unknown[]; operation_receipts?: OfflineOperationReceipt[] }> {
  return client.request(
    `${projBase(orgId, projectId)}/field/packages/${encodeURIComponent(packageId)}/sync`,
    { method: "POST", body: JSON.stringify(body), headers: csrfHeaders(csrf) }
  );
}

// R014 Extensions
export function listOfficialExtensions(
  client: ApiClient,
  orgId: string
): Promise<{ items: OfficialExtensionCatalogItem[] }> {
  return client.request<{ items: OfficialExtensionCatalogItem[] }>(
    `/organizations/${encodeURIComponent(orgId)}/extensions/official`
  );
}

export function registerOfficialExtension(
  client: ApiClient,
  orgId: string,
  extensionKey: string,
  csrf: string
): Promise<ExtensionManifest> {
  return client.request<ExtensionManifest>(
    `/organizations/${encodeURIComponent(orgId)}/extensions/official/${encodeURIComponent(extensionKey)}/register`,
    { method: "POST", headers: csrfHeaders(csrf) }
  );
}

export function activateProjectExtension(
  client: ApiClient,
  orgId: string,
  projectId: string,
  manifestId: string,
  csrf: string
): Promise<ExtensionActivation> {
  return activateExtension(client, orgId, projectId, manifestId, csrf);
}

export function disableProjectExtension(
  client: ApiClient,
  orgId: string,
  projectId: string,
  extensionKey: string,
  csrf: string
): Promise<ExtensionActivation> {
  return client.request<ExtensionActivation>(
    `${projBase(orgId, projectId)}/extensions/${encodeURIComponent(extensionKey)}/disable`,
    { method: "POST", headers: csrfHeaders(csrf) }
  );
}

export function listExtensionManifests(
  client: ApiClient,
  orgId: string
): Promise<{ items: ExtensionManifest[] }> {
  return client.request<{ items: ExtensionManifest[] }>(
    `/organizations/${encodeURIComponent(orgId)}/extensions/manifests`
  );
}

export function listProjectActivations(
  client: ApiClient,
  orgId: string,
  projectId: string
): Promise<{ items: ExtensionActivation[] }> {
  return client.request(`${projBase(orgId, projectId)}/extensions/activations`);
}

function extensionBase(orgId: string, projectId: string, key: string): string {
  return `${projBase(orgId, projectId)}/extensions/${encodeURIComponent(key)}`;
}

export function activateExtension(client: ApiClient, orgId: string, projectId: string, manifestId: string, csrf: string): Promise<ExtensionActivation> {
  return client.request(`${projBase(orgId, projectId)}/extensions/activations`, {
    method: "POST", body: JSON.stringify({manifest_id: manifestId}), headers: csrfHeaders(csrf)
  });
}
export function deactivateExtension(client: ApiClient, orgId: string, projectId: string, activationId: string, csrf: string): Promise<ExtensionActivation> {
  return client.request(`${projBase(orgId, projectId)}/extensions/activations/${encodeURIComponent(activationId)}/deactivate`, {
    method: "POST", headers: csrfHeaders(csrf)
  });
}
export function transitionExtension(client: ApiClient, orgId: string, projectId: string, activationId: string, expectedManifest: string, targetManifest: string, csrf: string): Promise<ExtensionActivation> {
  return client.request(`${projBase(orgId, projectId)}/extensions/activations/${encodeURIComponent(activationId)}/transition`, {
    method: "POST", body: JSON.stringify({expected_manifest_id: expectedManifest, target_manifest_id: targetManifest}), headers: csrfHeaders(csrf)
  });
}
export function getExtensionActivationHistory(client: ApiClient, orgId: string, projectId: string, activationId: string): Promise<{items: ExtensionLifecycleEvent[]}> {
  return client.request(`${projBase(orgId, projectId)}/extensions/activations/${encodeURIComponent(activationId)}/history`);
}
export function listExtensionRecords(client: ApiClient, orgId: string, projectId: string, key: string): Promise<{items: ExtensionRecord[]}> {
  return client.request(`${extensionBase(orgId, projectId, key)}/records`);
}
export function createExtensionRecord(client: ApiClient, orgId: string, projectId: string, key: string, body: {
  entity_name: string; payload: ExtensionPayload; linked_layer_id?: string; linked_feature_id?: string;
  expected_link_context?: ExtensionLinkContext;
}, csrf: string): Promise<ExtensionRecord> {
  return client.request(`${extensionBase(orgId, projectId, key)}/records`, {
    method: "POST", body: JSON.stringify(body), headers: csrfHeaders(csrf)
  });
}
export function updateExtensionRecord(client: ApiClient, orgId: string, projectId: string, key: string, id: string, body: {
  expected_revision: number; expected_manifest_id: string; payload: ExtensionPayload;
  expected_link_context?: ExtensionLinkContext;
}, csrf: string): Promise<ExtensionRecord> {
  return client.request(`${extensionBase(orgId, projectId, key)}/records/${encodeURIComponent(id)}`, {
    method: "PUT", body: JSON.stringify(body), headers: csrfHeaders(csrf)
  });
}
export function getExtensionRecordHistory(client: ApiClient, orgId: string, projectId: string, key: string, id: string): Promise<{items: ExtensionRevision[]}> {
  return client.request(`${extensionBase(orgId, projectId, key)}/records/${encodeURIComponent(id)}/history`);
}
export function exportExtensionBundle(client: ApiClient, orgId: string, projectId: string, key: string): Promise<ExtensionPortableBundle> {
  return client.request(`${extensionBase(orgId, projectId, key)}/portable-export`);
}
export function importExtensionBundle(client: ApiClient, orgId: string, projectId: string, key: string, body: ExtensionPortableBundle & {expected_manifest_id: string}, csrf: string): Promise<ExtensionImportReceipt> {
  return client.request(`${extensionBase(orgId, projectId, key)}/portable-import`, {
    method: "POST", body: JSON.stringify(body), headers: csrfHeaders(csrf)
  });
}

// R015 Operations
export function getOpsHealthReady(
  client: ApiClient
): Promise<OpsHealthReadiness> {
  return client.request("/ops/health/ready");
}

export function getOpsReleaseCapabilities(
  client: ApiClient
): Promise<ReleaseCapabilityReport> {
  return client.request("/ops/release-capabilities");
}

export function getOrgEntitlement(
  client: ApiClient,
  orgId: string
): Promise<{
  deployment_profile: string;
  plan_tier: string;
  max_projects: number;
  max_storage_mb: number;
  authorization_distinct: boolean;
  data_deletion_on_plan_change: boolean;
  /** Optional commercial configuration; no quota/feature enforcement. */
  enforcement: "metadata_only";
}> {
  return client.request(`/organizations/${encodeURIComponent(orgId)}/ops/entitlement`);
}
