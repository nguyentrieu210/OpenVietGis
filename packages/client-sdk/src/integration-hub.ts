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
  | "read_only_export";

export type ActionReason = ActionDenialReason;

export type AdapterExecutionStatus =
  | "existing_read_only_api"
  | "read_only_profile_ready"
  | "candidate_external_branch"
  | "planned_not_implemented";

export type IntegrationExportFormat = "geopackage" | "geojson" | "postgis_sql";

export type IntegrationExportAdapterKey = "qgis" | "postgis";

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
  method: "GET";
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
  return "gpkg";
}

function sanitizeFilename(raw: string): string | null {
  const withoutControls = raw.replace(/[\x00-\x1f\x7f]/g, "").trim();
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
 * Build the encoded read-only export path for a QGIS or PostGIS layer snapshot.
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
 * Download a read-only QGIS (.gpkg/.geojson) or PostGIS (.sql/.gpkg) layer
 * snapshot via HTTP GET with explicit CRS and integrity response metadata.
 * Strictly read-only; never issues write or credential requests.
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
    featureCount: parseFeatureCountHeader(response.headers)
  };
}
