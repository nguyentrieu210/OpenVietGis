import type { ApiClient } from "./http";

export type ConnectorLifecycle = "existing_read_only" | "candidate" | "planned";

export type ConnectorActionName =
  | "catalog.discover"
  | "connection.inspect"
  | "connection.configure"
  | "data.import"
  | "data.export"
  | "sync.pull"
  | "sync.push"
  | "sync.approve";

export type ActionDenialReason =
  | "permission_denied"
  | "unsupported_by_adapter"
  | "not_implemented"
  | "catalog_only"
  | "read_only_export"
  | "controlled_changeset_sync";

export type ActionReason = ActionDenialReason;

export type AdapterExecutionStatus =
  | "existing_read_only_api"
  | "read_only_profile_ready"
  | "candidate_external_branch"
  | "planned_not_implemented";

export type IntegrationExportFormat =
  | "geopackage"
  | "geojson"
  | "postgis_sql"
  | "dxf";

export type IntegrationExportAdapterKey = "qgis" | "postgis" | "autocad";

export interface IntegrationActionEvaluation {
  action: ConnectorActionName;
  required_permission: string;
  eligible_by_role: boolean;
  executable_in_hub: boolean;
  allowed: boolean;
  reason: ActionDenialReason;
}

export interface IntegrationAdapterSummary {
  key: string;
  name: string;
  category: string;
  mode: string;
  lifecycle: ConnectorLifecycle;
  protocols: string[];
  note: string;
  existing_api: string | null;
  actions: IntegrationActionEvaluation[];
}

export interface IntegrationCatalogResponse {
  schema_version: number;
  organization_id: string;
  project_id: string;
  mode: "catalog_read_only";
  default_write_policy: "deny";
  scoped_access: boolean;
  adapters: IntegrationAdapterSummary[];
}

export interface IntegrationEndpointDescriptor {
  name: string;
  method: "GET" | "POST";
  path_template: string;
  required_permission: string;
}

export interface ReadOnlyConnectionProfile {
  adapter_key: string;
  access_mode: "read_only_exchange";
  supports_direct_db_credentials: boolean;
  requires_per_layer_acl: boolean;
  default_write_policy: "deny";
  supported_protocols: string[];
  export_formats?: IntegrationExportFormat[];
  existing_endpoints: IntegrationEndpointDescriptor[];
  crs_policy: "explicit_only_never_inferred";
  restrictions: string[];
  sync_push_enabled?: boolean;
  deletions_blocked_by_default?: boolean;
}

export interface AdapterLayerDescriptor {
  layer_id: string;
  source_id: string;
  name: string;
  geometry_family: string;
  dimensions: string;
  validation_state: string;
  source_crs_code: number | null;
  stored_crs_code: number | null;
  crs_explicit: boolean;
  layer_revision: number;
  content_revision: string;
  schema_revision: string;
  export_permitted: boolean;
}

export interface IntegrationAdapterProfileResponse {
  schema_version: number;
  organization_id: string;
  project_id: string;
  scoped_access: boolean;
  default_write_policy: "deny";
  adapter: IntegrationAdapterSummary;
  execution_status: AdapterExecutionStatus;
  connection_profile: ReadOnlyConnectionProfile | null;
  layers: AdapterLayerDescriptor[];
}

export interface IntegrationLayerExportResult {
  blob: Blob;
  filename: string;
  contentType: string;
  sha256: string | null;
  crs: string | null;
  contentRevision: string | null;
  schemaRevision: string | null;
  featureCount: number | null;
  attributeLoss: boolean;
}

export interface IntegrationSyncOperationPreview {
  op_type: "create" | "update";
  feature_id: string;
  changed_fields: string[];
  geometry_changed: boolean;
}

export interface IntegrationSyncPreviewResponse {
  schema_version: number;
  adapter_key: string;
  layer_id: string;
  source_id: string;
  stored_crs_code: number;
  geometry_family: "Point" | "LineString" | "Polygon";
  base_content_revision: string;
  current_content_revision: string;
  base_schema_revision: string;
  current_schema_revision: string;
  stale_revision: boolean;
  deletions_blocked: boolean;
  create_count: number;
  update_count: number;
  unchanged_count: number;
  omitted_server_feature_count: number;
  omitted_server_feature_ids: string[];
  diff_sha256: string;
  can_commit: boolean;
  operations: IntegrationSyncOperationPreview[];
}

export interface IntegrationSyncPushResponse {
  changeset_id: string;
  source_id: string;
  layer_id: string;
  operation_id: string;
  resulting_content_revision: string;
  resulting_catalog_revision: number;
  operation_count: number;
  create_count: number;
  update_count: number;
  omitted_server_feature_count: number;
  deletions_blocked: boolean;
  diff_sha256: string;
  audit_event_id: string;
}

export interface QgisGpkgSyncPreviewInput {
  file: Blob;
  filename?: string;
  baseContentRevision: string;
  baseSchemaRevision: string;
  csrfToken?: string;
}

export interface QgisGpkgSyncPushInput {
  file: Blob;
  filename?: string;
  baseContentRevision: string;
  baseSchemaRevision: string;
  operationId: string;
  expectedDiffSha256: string;
  summary?: string;
  csrfToken?: string;
}

function integrationHubRoot(organizationId: string, projectId: string): string {
  return (
    "/organizations/" +
    encodeURIComponent(organizationId) +
    "/projects/" +
    encodeURIComponent(projectId) +
    "/integration-hub"
  );
}

function extensionForExportFormat(format: IntegrationExportFormat): string {
  if (format === "geojson") {
    return "geojson";
  }
  if (format === "postgis_sql") {
    return "sql";
  }
  if (format === "dxf") {
    return "dxf";
  }
  return "gpkg";
}

function sanitizeFilename(raw: string): string | null {
  const withoutControls = Array.from(raw)
    .filter((ch) => {
      const code = ch.charCodeAt(0);
      return code >= 0x20 && code !== 0x7f;
    })
    .join("")
    .trim();
  if (!withoutControls) {
    return null;
  }
  const segments = withoutControls.split(/[\\/]/);
  const basename = (segments[segments.length - 1] ?? "")
    .replace(/^["']+|["']+$/g, "")
    .trim();
  if (!basename || basename === "." || basename === "..") {
    return null;
  }
  return basename;
}

function resolveUploadFilename(file: Blob, explicitFilename?: string): string {
  const candidate =
    explicitFilename ??
    (typeof File !== "undefined" && file instanceof File ? file.name : "");
  const cleaned = sanitizeFilename(candidate);
  return cleaned ?? "layer.gpkg";
}

function parseContentDispositionFilename(
  header: string | null,
  fallback: string
): string {
  if (!header) {
    return fallback;
  }

  const extMatch = /filename\*\s*=\s*(?:UTF-8''|utf-8'')?([^;\r\n]+)/i.exec(
    header
  );
  if (extMatch?.[1]) {
    const rawValue = extMatch[1].trim().replace(/^["']|["']$/g, "");
    try {
      const decoded = decodeURIComponent(rawValue);
      const cleaned = sanitizeFilename(decoded);
      if (cleaned) {
        return cleaned;
      }
    } catch {
      const cleaned = sanitizeFilename(rawValue);
      if (cleaned) {
        return cleaned;
      }
    }
  }

  const quotedMatch = /filename\s*=\s*"([^"\r\n]+)"/i.exec(header);
  if (quotedMatch?.[1]) {
    const cleaned = sanitizeFilename(quotedMatch[1]);
    if (cleaned) {
      return cleaned;
    }
  }

  const unquotedMatch = /filename\s*=\s*([^;\r\n]+)/i.exec(header);
  if (unquotedMatch?.[1]) {
    const cleaned = sanitizeFilename(unquotedMatch[1]);
    if (cleaned) {
      return cleaned;
    }
  }

  return fallback;
}

function parseNonEmptyHeader(headers: Headers, name: string): string | null {
  const value = headers.get(name)?.trim();
  return value ? value : null;
}

function parseFeatureCountHeader(headers: Headers): number | null {
  const raw = headers.get("x-openvietgis-feature-count")?.trim();
  if (!raw || !/^\d+$/.test(raw)) {
    return null;
  }
  const parsed = Number.parseInt(raw, 10);
  return Number.isSafeInteger(parsed) ? parsed : null;
}

function parseBooleanHeader(headers: Headers, name: string): boolean {
  return headers.get(name)?.trim().toLowerCase() === "true";
}

function buildSyncRequestInit(form: FormData, csrfToken?: string): RequestInit {
  const init: RequestInit = {
    method: "POST",
    body: form,
    cache: "no-store"
  };
  const explicitCsrf = csrfToken?.trim();
  if (explicitCsrf) {
    init.headers = { "x-openvietgis-csrf": explicitCsrf };
  }
  return init;
}

/**
 * Read the project-scoped, read-only Integration Hub adapter catalog.
 * Never mutates state or binds external credentials.
 */
export function getIntegrationHubCatalog(
  client: ApiClient,
  organizationId: string,
  projectId: string
): Promise<IntegrationCatalogResponse> {
  return client.request<IntegrationCatalogResponse>(
    integrationHubRoot(organizationId, projectId) + "/catalog",
    { method: "GET" }
  );
}

/**
 * Inspect a single adapter's read-only connection profile, visible layers,
 * and fail-closed action policy matrix within the project scope.
 */
export function getIntegrationHubAdapterProfile(
  client: ApiClient,
  organizationId: string,
  projectId: string,
  adapterKey: string
): Promise<IntegrationAdapterProfileResponse> {
  return client.request<IntegrationAdapterProfileResponse>(
    integrationHubRoot(organizationId, projectId) +
      "/adapters/" +
      encodeURIComponent(adapterKey),
    { method: "GET" }
  );
}

/**
 * Build the encoded read-only export path for a QGIS, PostGIS, or AutoCAD DXF
 * layer snapshot.
 */
export function buildIntegrationLayerExportPath(
  organizationId: string,
  projectId: string,
  adapterKey: IntegrationExportAdapterKey,
  layerId: string,
  format: IntegrationExportFormat = "geopackage"
): string {
  return (
    integrationHubRoot(organizationId, projectId) +
    "/adapters/" +
    encodeURIComponent(adapterKey) +
    "/layers/" +
    encodeURIComponent(layerId) +
    "/export?format=" +
    encodeURIComponent(format)
  );
}

/**
 * Download a read-only QGIS (.gpkg/.geojson), PostGIS (.sql/.gpkg), or
 * AutoCAD (.dxf) layer snapshot via HTTP GET with explicit CRS, attribute-loss,
 * and integrity response metadata. Strictly read-only.
 */
export async function downloadIntegrationLayerExport(
  client: ApiClient,
  organizationId: string,
  projectId: string,
  adapterKey: IntegrationExportAdapterKey,
  layerId: string,
  format: IntegrationExportFormat = "geopackage"
): Promise<IntegrationLayerExportResult> {
  const path = buildIntegrationLayerExportPath(
    organizationId,
    projectId,
    adapterKey,
    layerId,
    format
  );
  const response = await client.requestRaw(path, {
    method: "GET",
    cache: "no-store"
  });
  const blob = await response.blob();
  const fallbackFilename =
    `${adapterKey}-${layerId}.${extensionForExportFormat(format)}`;
  const filename = parseContentDispositionFilename(
    response.headers.get("content-disposition"),
    fallbackFilename
  );
  const contentType =
    parseNonEmptyHeader(response.headers, "content-type") ??
    (blob.type || "application/octet-stream");

  return {
    blob,
    filename,
    contentType,
    sha256: parseNonEmptyHeader(response.headers, "x-content-sha256"),
    crs: parseNonEmptyHeader(response.headers, "x-openvietgis-crs"),
    contentRevision: parseNonEmptyHeader(
      response.headers,
      "x-openvietgis-content-revision"
    ),
    schemaRevision: parseNonEmptyHeader(
      response.headers,
      "x-openvietgis-schema-revision"
    ),
    featureCount: parseFeatureCountHeader(response.headers),
    attributeLoss: parseBooleanHeader(
      response.headers,
      "x-openvietgis-attribute-loss"
    )
  };
}

/**
 * Step 1 of controlled QGIS GeoPackage synchronization: upload an edited .gpkg
 * file against pinned layer revisions to compute a non-mutating diff preview
 * and diff_sha256 commitment hash. Deletions remain blocked by default.
 */
export function previewQgisGpkgSync(
  client: ApiClient,
  organizationId: string,
  projectId: string,
  layerId: string,
  input: QgisGpkgSyncPreviewInput
): Promise<IntegrationSyncPreviewResponse> {
  const form = new FormData();
  form.append(
    "file",
    input.file,
    resolveUploadFilename(input.file, input.filename)
  );
  form.append("base_content_revision", input.baseContentRevision);
  form.append("base_schema_revision", input.baseSchemaRevision);

  const path =
    integrationHubRoot(organizationId, projectId) +
    "/adapters/qgis/layers/" +
    encodeURIComponent(layerId) +
    "/sync-preview";

  return client.request<IntegrationSyncPreviewResponse>(
    path,
    buildSyncRequestInit(form, input.csrfToken)
  );
}

/**
 * Step 2 of controlled QGIS GeoPackage synchronization: commit a previously
 * previewed diff through the audited Changeset pipeline using an explicit
 * non-field-sync operationId and expectedDiffSha256 hash.
 */
export async function pushQgisGpkgSync(
  client: ApiClient,
  organizationId: string,
  projectId: string,
  layerId: string,
  input: QgisGpkgSyncPushInput
): Promise<IntegrationSyncPushResponse> {
  const operationId = input.operationId.trim();
  if (!operationId) {
    throw new Error("QGIS sync push requires a non-empty operationId.");
  }
  if (operationId.toLowerCase().startsWith("field-sync-")) {
    throw new Error(
      "QGIS sync push operationId must not use the reserved 'field-sync-' prefix."
    );
  }
  const expectedDiffSha256 = input.expectedDiffSha256.trim();
  if (!expectedDiffSha256) {
    throw new Error(
      "QGIS sync push requires expectedDiffSha256 from a preceding sync preview."
    );
  }

  const form = new FormData();
  form.append(
    "file",
    input.file,
    resolveUploadFilename(input.file, input.filename)
  );
  form.append("base_content_revision", input.baseContentRevision);
  form.append("base_schema_revision", input.baseSchemaRevision);
  form.append("operation_id", operationId);
  form.append("expected_diff_sha256", expectedDiffSha256);
  const summary = input.summary?.trim() || "QGIS GeoPackage sync push";
  form.append("summary", summary);

  const path =
    integrationHubRoot(organizationId, projectId) +
    "/adapters/qgis/layers/" +
    encodeURIComponent(layerId) +
    "/sync-push";

  return client.request<IntegrationSyncPushResponse>(
    path,
    buildSyncRequestInit(form, input.csrfToken)
  );
}

/**
 * R027 QGIS binding metadata, draft only: no agent pairing or writable sync.
 * An active connection MUST NOT be inferred from the presence of a binding.
 */
export interface IntegrationBindingDraft {
  id: string;
  layer_id: string;
  adapter_key: "qgis";
  external_dataset_ref: string;
  direction: "bidirectional_manual";
  source_crs_code: number;
  target_crs_code: number;
  external_primary_key: "ovg_feature_id";
  allowed_field_ids: string[];
  base_content_revision: string;
  base_schema_revision: string;
  state: "draft" | "active" | "paused" | "revoked";
  revision: number;
  created_at: string;
  updated_at: string;
  activated_at?: string | null;
  last_paired_at?: string | null;
  open_conflicts_count?: number;
  writes_allowed: boolean;
  deletes_allowed: false;
  connected: boolean;
  reason: string;
}

export interface CreateQgisBindingDraft {
  layer_id: string;
  adapter_key: "qgis";
  external_dataset_ref: string;
  direction: "bidirectional_manual";
  source_crs_code: number;
  target_crs_code: number;
  external_primary_key: "ovg_feature_id";
  allowed_field_ids: string[];
}

export function listIntegrationBindingDrafts(
  client: ApiClient,
  organizationId: string,
  projectId: string
): Promise<{ items: IntegrationBindingDraft[] }> {
  return client.request<{ items: IntegrationBindingDraft[] }>(
    integrationHubRoot(organizationId, projectId) + "/bindings",
    { method: "GET", cache: "no-store" }
  );
}

/** Creates a disabled QGIS binding: not a connection activation or data write. */
export function createQgisBindingDraft(
  client: ApiClient,
  organizationId: string,
  projectId: string,
  input: CreateQgisBindingDraft,
  csrfToken?: string
): Promise<IntegrationBindingDraft> {
  return client.request<IntegrationBindingDraft>(
    integrationHubRoot(organizationId, projectId) + "/bindings",
    {
      method: "POST",
      headers: csrfToken?.trim() ? { "x-openvietgis-csrf": csrfToken.trim() } : {},
      body: JSON.stringify(input)
    }
  );
}

/** Non-activating pause or permanent revoke with optimistic revision check. */
export async function changeQgisBindingDraftLifecycle(
  client: ApiClient,
  organizationId: string,
  projectId: string,
  bindingId: string,
  action: "pause" | "revoke",
  expectedRevision: number,
  csrfToken?: string
): Promise<IntegrationBindingDraft> {
  if (!Number.isSafeInteger(expectedRevision) || expectedRevision < 1) {
    throw new Error("Expected a positive integer binding revision.");
  }
  return client.request<IntegrationBindingDraft>(
    integrationHubRoot(organizationId, projectId) +
      "/bindings/" + encodeURIComponent(bindingId) + "/lifecycle",
    {
      method: "POST",
      headers: csrfToken?.trim() ? { "x-openvietgis-csrf": csrfToken.trim() } : {},
      body: JSON.stringify({ action, expected_revision: expectedRevision })
    }
  );
}

/** Eligible verified 2D layers/fields for a QGIS binding draft (manager-only). */
export interface QgisBindingCandidate {
  layer_id: string;
  source_id?: string;
  layer_name: string;
  stored_crs_code: number;
  geometry_family: "Point" | "LineString" | "Polygon";
  allowed_field_ids: string[];
  binding_status: "draft_only";
}

export function listQgisBindingCandidates(
  client: ApiClient,
  organizationId: string,
  projectId: string
): Promise<{ items: QgisBindingCandidate[] }> {
  return client.request<{ items: QgisBindingCandidate[] }>(
    integrationHubRoot(organizationId, projectId) + "/bindings/candidates",
    { method: "GET", cache: "no-store" }
  );
}

/** R027 Pha A: Explicitly activate a draft or paused QGIS binding. */
export function activateQgisBinding(
  client: ApiClient,
  organizationId: string,
  projectId: string,
  bindingId: string,
  expectedRevision: number,
  csrfToken?: string
): Promise<IntegrationBindingDraft> {
  if (!Number.isSafeInteger(expectedRevision) || expectedRevision < 1) {
    throw new Error("Expected a positive integer binding revision.");
  }
  return client.request<IntegrationBindingDraft>(
    integrationHubRoot(organizationId, projectId) +
      "/bindings/" +
      encodeURIComponent(bindingId) +
      "/activate",
    {
      method: "POST",
      headers: csrfToken?.trim() ? { "x-openvietgis-csrf": csrfToken.trim() } : {},
      body: JSON.stringify({ expected_revision: expectedRevision })
    }
  );
}

export interface BindingPairingToken {
  id: string;
  binding_id: string;
  label: string;
  token_prefix: string;
  expires_at: string;
  last_used_at: string | null;
  revoked_at: string | null;
  created_at: string;
}

export interface IssuedPairingTokenResponse {
  token: BindingPairingToken;
  binding: IntegrationBindingDraft;
  pairing_code: string;
  bearer_token: string;
}

export function listBindingPairingTokens(
  client: ApiClient,
  organizationId: string,
  projectId: string,
  bindingId: string
): Promise<{ items: BindingPairingToken[] }> {
  return client.request<{ items: BindingPairingToken[] }>(
    integrationHubRoot(organizationId, projectId) +
      "/bindings/" +
      encodeURIComponent(bindingId) +
      "/pairing-tokens",
    { method: "GET", cache: "no-store" }
  );
}

export function issueBindingPairingToken(
  client: ApiClient,
  organizationId: string,
  projectId: string,
  bindingId: string,
  input: { label?: string; expires_in_days?: number },
  csrfToken?: string
): Promise<IssuedPairingTokenResponse> {
  return client.request<IssuedPairingTokenResponse>(
    integrationHubRoot(organizationId, projectId) +
      "/bindings/" +
      encodeURIComponent(bindingId) +
      "/pairing-tokens",
    {
      method: "POST",
      headers: csrfToken?.trim() ? { "x-openvietgis-csrf": csrfToken.trim() } : {},
      body: JSON.stringify(input)
    }
  );
}

export function revokeBindingPairingToken(
  client: ApiClient,
  organizationId: string,
  projectId: string,
  bindingId: string,
  tokenId: string,
  csrfToken?: string
): Promise<BindingPairingToken> {
  return client.request<BindingPairingToken>(
    integrationHubRoot(organizationId, projectId) +
      "/bindings/" +
      encodeURIComponent(bindingId) +
      "/pairing-tokens/" +
      encodeURIComponent(tokenId) +
      "/revoke",
    {
      method: "POST",
      headers: csrfToken?.trim() ? { "x-openvietgis-csrf": csrfToken.trim() } : {}
    }
  );
}

/** R027 Pha B: Per-feature conflict ledger and resolution. */
export interface SyncConflictRecord {
  id: string;
  binding_id: string;
  layer_id: string;
  feature_id: string;
  conflict_kind: "attribute" | "geometry" | "delete_candidate";
  field_name: string | null;
  baseline_value: unknown;
  web_value: unknown;
  qgis_value: unknown;
  status: "open" | "resolved_web" | "resolved_qgis" | "resolved_merged" | "dismissed";
  resolved_value: unknown;
  resolution_note: string | null;
  created_at: string;
  resolved_at: string | null;
}

export function listBindingConflicts(
  client: ApiClient,
  organizationId: string,
  projectId: string,
  bindingId: string
): Promise<{ items: SyncConflictRecord[] }> {
  return client.request<{ items: SyncConflictRecord[] }>(
    integrationHubRoot(organizationId, projectId) +
      "/bindings/" +
      encodeURIComponent(bindingId) +
      "/conflicts",
    { method: "GET", cache: "no-store" }
  );
}

export function resolveBindingConflict(
  client: ApiClient,
  organizationId: string,
  projectId: string,
  bindingId: string,
  conflictId: string,
  input: {
    decision: "keep_web" | "keep_qgis" | "merge" | "dismiss";
    merged_value?: unknown;
    note?: string;
  },
  csrfToken?: string
): Promise<SyncConflictRecord> {
  return client.request<SyncConflictRecord>(
    integrationHubRoot(organizationId, projectId) +
      "/bindings/" +
      encodeURIComponent(bindingId) +
      "/conflicts/" +
      encodeURIComponent(conflictId) +
      "/resolve",
    {
      method: "POST",
      headers: csrfToken?.trim() ? { "x-openvietgis-csrf": csrfToken.trim() } : {},
      body: JSON.stringify(input)
    }
  );
}
