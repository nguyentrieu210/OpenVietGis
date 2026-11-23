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
  | "catalog_only";

export type AdapterExecutionStatus =
  | "existing_read_only_api"
  | "read_only_profile_ready"
  | "candidate_external_branch"
  | "planned_not_implemented";

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

function integrationHubRoot(organizationId: string, projectId: string): string {
  return (
    "/organizations/" +
    encodeURIComponent(organizationId) +
    "/projects/" +
    encodeURIComponent(projectId) +
    "/integration-hub"
  );
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
