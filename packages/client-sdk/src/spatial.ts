import { ApiClient, ApiError, projectApiPath } from "./http";
import type { components, operations } from "./generated/spatial-catalog";

export type SpatialSource = components["schemas"]["Source"];
export type SpatialSourceList = components["schemas"]["SourceList"];
export type SpatialSourceMutation = components["schemas"]["SourceMutation"];
export type RegisterSpatialSourceInput = components["schemas"]["RegisterSource"];
export type SpatialDeclaredCrs = components["schemas"]["DeclaredCRS"];
export type RetireSpatialSourceInput = components["schemas"]["RetireResource"];
export type SpatialErrorCode = components["schemas"]["SpatialError"]["code"];

// OpenAPI anyOf's required-only branches are emitted as unknown by the generator.
// Keep the exact contract and require at least one explicit change in the SDK.
export type UpdateSpatialSourceInput = { expected_revision: number } & (
  | { name: string; declared_crs?: SpatialDeclaredCrs }
  | { name?: string; declared_crs: SpatialDeclaredCrs }
);

const spatialCodes: ReadonlySet<string> = new Set<SpatialErrorCode>([
  "STALE_CATALOG_REVISION", "STALE_LAYER_REVISION", "STALE_READ_PIN",
  "ACTIVE_LAYER_DEPENDENCIES", "SOURCE_ALREADY_REGISTERED", "SOURCE_NOT_READY",
  "PROJECT_NOT_WRITABLE", "UNKNOWN_REFERENCE_KEY", "NO_CHANGES",
  "CRS_CORRECTION_UNVERIFIED", "UNSUPPORTED_CRS", "UNSUPPORTED_TRANSFORMATION",
  "TRANSFORMATION_OUT_OF_AREA", "TRANSFORMATION_UNAVAILABLE", "INVALID_CURSOR",
  "INVALID_GEOMETRY", "BOUNDS_EXCEEDED"
]);

export class SpatialApiError extends ApiError {
  constructor(status: number, response: Response, public readonly code: SpatialErrorCode) {
    super(status, response);
    this.name = "SpatialApiError";
  }
}

async function spatialRequest<T>(client: ApiClient, path: string, init?: RequestInit): Promise<T> {
  try {
    return await client.request<T>(path, init);
  } catch (error) {
    if (error instanceof ApiError) {
      const payload = await error.readPayload();
      if (payload.code && spatialCodes.has(payload.code)) {
        throw new SpatialApiError(error.status, error.response, payload.code as SpatialErrorCode);
      }
    }
    throw error;
  }
}

function sourceRoot(organizationId: string, projectId: string): string {
  return projectApiPath(organizationId, projectId) + "/spatial/sources";
}

function sourcePath(organizationId: string, projectId: string, sourceId: string): string {
  return sourceRoot(organizationId, projectId) + "/" + encodeURIComponent(sourceId);
}

export function listSpatialSources(
  client: ApiClient, organizationId: string, projectId: string,
  options: { limit?: number; after_id?: string } = {}
): Promise<SpatialSourceList> {
  const params = new URLSearchParams();
  if (options.limit !== undefined) params.set("limit", String(options.limit));
  if (options.after_id !== undefined) params.set("after_id", options.after_id);
  const query = params.toString();
  return spatialRequest(client, sourceRoot(organizationId, projectId) + (query ? "?" + query : ""));
}

export function getSpatialSource(
  client: ApiClient, organizationId: string, projectId: string, sourceId: string
): Promise<SpatialSource> {
  return spatialRequest(client, sourcePath(organizationId, projectId, sourceId));
}

export function registerSpatialSource(
  client: ApiClient, organizationId: string, projectId: string,
  input: RegisterSpatialSourceInput
): Promise<SpatialSourceMutation> {
  return spatialRequest(client, sourceRoot(organizationId, projectId), {
    method: "POST", body: JSON.stringify(input)
  });
}

export function updateSpatialSource(
  client: ApiClient, organizationId: string, projectId: string, sourceId: string,
  input: UpdateSpatialSourceInput
): Promise<SpatialSourceMutation> {
  return spatialRequest(client, sourcePath(organizationId, projectId, sourceId), {
    method: "PATCH", body: JSON.stringify(input)
  });
}

export function retireSpatialSource(
  client: ApiClient, organizationId: string, projectId: string, sourceId: string,
  input: RetireSpatialSourceInput
): Promise<SpatialSourceMutation> {
  return spatialRequest(client, sourcePath(organizationId, projectId, sourceId) + "/retire", {
    method: "POST", body: JSON.stringify(input)
  });
}

export type SpatialLayer = components["schemas"]["Layer"];
export type SpatialLayerList = components["schemas"]["LayerList"];
export type SpatialLayerMutation = components["schemas"]["LayerMutation"];
export type CreateSpatialLayerInput = components["schemas"]["CreateLayer"];
export type RenameSpatialLayerInput = components["schemas"]["RenameLayer"];

function layerRoot(organizationId: string, projectId: string): string {
  return projectApiPath(organizationId, projectId) + "/layers";
}

function layerPath(organizationId: string, projectId: string, layerId: string): string {
  return layerRoot(organizationId, projectId) + "/" + encodeURIComponent(layerId);
}

export function listSpatialLayers(
  client: ApiClient, organizationId: string, projectId: string,
  options: { limit?: number; after_id?: string } = {}
): Promise<SpatialLayerList> {
  const params = new URLSearchParams();
  if (options.limit !== undefined) params.set("limit", String(options.limit));
  if (options.after_id !== undefined) params.set("after_id", options.after_id);
  const query = params.toString();
  return spatialRequest(client, layerRoot(organizationId, projectId) + (query ? "?" + query : ""));
}

export function getSpatialLayer(
  client: ApiClient, organizationId: string, projectId: string, layerId: string
): Promise<SpatialLayer> {
  return spatialRequest(client, layerPath(organizationId, projectId, layerId));
}

export function createSpatialLayer(
  client: ApiClient, organizationId: string, projectId: string,
  input: CreateSpatialLayerInput
): Promise<SpatialLayerMutation> {
  return spatialRequest(client, layerRoot(organizationId, projectId), {
    method: "POST", body: JSON.stringify(input)
  });
}

export function renameSpatialLayer(
  client: ApiClient, organizationId: string, projectId: string, layerId: string,
  input: RenameSpatialLayerInput
): Promise<SpatialLayerMutation> {
  return spatialRequest(client, layerPath(organizationId, projectId, layerId), {
    method: "PATCH", body: JSON.stringify(input)
  });
}

export function retireSpatialLayer(
  client: ApiClient, organizationId: string, projectId: string, layerId: string,
  input: RetireSpatialSourceInput
): Promise<SpatialLayerMutation> {
  return spatialRequest(client, layerPath(organizationId, projectId, layerId) + "/retire", {
    method: "POST", body: JSON.stringify(input)
  });
}

export type SpatialCatalogChange = components["schemas"]["CatalogChange"];
export type SpatialChangeList = components["schemas"]["ChangeList"];

export function listSpatialCatalogChanges(
  client: ApiClient, organizationId: string, projectId: string,
  options: NonNullable<operations["listSpatialCatalogChanges"]["parameters"]["query"]> = {}
): Promise<SpatialChangeList> {
  const params = new URLSearchParams();
  if (options.limit !== undefined) params.set("limit", String(options.limit));
  if (options.after_id !== undefined) params.set("after_id", options.after_id);
  const query = params.toString();
  const path = "/organizations/" + encodeURIComponent(organizationId) +
    "/projects/" + encodeURIComponent(projectId) + "/spatial/changes";
  return spatialRequest(client, path + (query ? "?" + query : ""));
}

export type SpatialFeaturePage = components["schemas"]["FeaturePage"];
export type SpatialFeatureDetail = components["schemas"]["FeatureDetail"];
export type SpatialFeature = components["schemas"]["Feature"];
export type SpatialFeatureReadContext = components["schemas"]["FeatureReadContext"];
export type ReadSpatialFeaturesOptions = operations["pageSpatialFeatures"]["parameters"]["query"];
export type GetSpatialFeatureOptions = operations["getSpatialFeature"]["parameters"]["query"];

function featureReadParams(options: GetSpatialFeatureOptions): URLSearchParams {
  const params = new URLSearchParams({
    content_revision: options.content_revision,
    schema_revision: options.schema_revision
  });
  if (options.include_geometry !== undefined) {
    params.set("include_geometry", String(options.include_geometry));
  }
  if (options.response_crs !== undefined) params.set("response_crs", String(options.response_crs));
  return params;
}

export function readSpatialFeatures(
  client: ApiClient, organizationId: string, projectId: string, layerId: string,
  options: ReadSpatialFeaturesOptions
): Promise<SpatialFeaturePage> {
  const params = featureReadParams(options);
  if (options.limit !== undefined) params.set("limit", String(options.limit));
  if (options.after_feature_id !== undefined) params.set("after_feature_id", options.after_feature_id);
  return spatialRequest(client, layerPath(organizationId, projectId, layerId) +
    "/features?" + params.toString());
}

export function getSpatialFeature(
  client: ApiClient, organizationId: string, projectId: string, layerId: string,
  featureId: string, options: GetSpatialFeatureOptions
): Promise<SpatialFeatureDetail> {
  return spatialRequest(client, layerPath(organizationId, projectId, layerId) +
    "/features/" + encodeURIComponent(featureId) + "?" + featureReadParams(options).toString());
}

export type SpatialCrsRegistry = components["schemas"]["CRSRegistry"];
export type SpatialCrsProfile = components["schemas"]["CRSProfile"];
export type SpatialTransformationProfile = components["schemas"]["TransformationProfile"];

/** Finite scoped definitions and approved direct edges; no caller-supplied CRS. */
export function listSpatialCrsProfiles(
  client: ApiClient, organizationId: string, projectId: string
): Promise<SpatialCrsRegistry> {
  return spatialRequest(client, "/organizations/" + encodeURIComponent(organizationId) +
    "/projects/" + encodeURIComponent(projectId) + "/spatial/crs-profiles");
}
