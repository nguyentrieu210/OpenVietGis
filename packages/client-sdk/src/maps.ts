import { ApiClient, ApiError, projectApiPath } from "./http";
import type { components, operations } from "./generated/map-workspace";

export type MapViewpoint = components["schemas"]["MapViewpoint"];
export type MapStyle = components["schemas"]["MapStyle"];
export type MapLayerReference = components["schemas"]["MapLayerReference"];
export type MapConfiguration = components["schemas"]["MapConfiguration"];
export type ProjectMap = components["schemas"]["ProjectMap"];
export type DefaultViewpointIntent = components["schemas"]["DefaultViewpointIntent"];
export type SaveMapInput = components["schemas"]["SaveMapInput"];
export type MapSaveReceipt = components["schemas"]["MapSaveReceipt"];
export type MapRevision = components["schemas"]["MapRevision"];
export type MapHistoryPage = components["schemas"]["MapHistoryPage"];
export type MapViewportFeature = components["schemas"]["MapViewportFeature"];
export type MapViewport = components["schemas"]["MapViewport"];
export type MapDisplayExtent = components["schemas"]["MapDisplayExtent"];
export type MapBounds = components["schemas"]["Bounds4326"];
export type MapErrorCode = components["schemas"]["MapError"]["code"];
export type MapHistoryOptions = NonNullable<operations["listMapHistory"]["parameters"]["query"]>;
export type MapViewportOptions = NonNullable<operations["getMapViewport"]["parameters"]["query"]>;
export type MapDisplayExtentOptions = NonNullable<operations["getMapDisplayExtent"]["parameters"]["query"]>;

const mapCodes: ReadonlySet<string> = new Set<MapErrorCode>([
  "STALE_MAP_REVISION", "OPERATION_INTENT_CONFLICT", "NO_CHANGES", "MAP_READ_UNAVAILABLE",
  "STALE_READ_PIN", "PROJECT_NOT_WRITABLE", "SOURCE_NOT_READY", "UNSUPPORTED_CRS",
  "UNSUPPORTED_TRANSFORMATION", "TRANSFORMATION_OUT_OF_AREA", "TRANSFORMATION_UNAVAILABLE",
  "INVALID_GEOMETRY", "BOUNDS_EXCEEDED"
]);

export class MapApiError extends ApiError {
  constructor(status: number, response: Response, public readonly code: MapErrorCode) {
    super(status, response);
    this.name = "MapApiError";
  }
}

async function mapRequest<T>(client: ApiClient, path: string, init?: RequestInit): Promise<T> {
  try {
    return await client.request<T>(path, init);
  } catch (error) {
    if (error instanceof ApiError && [409, 422, 503].includes(error.status)) {
      const payload = await error.readPayload();
      if (payload.code && mapCodes.has(payload.code)) {
        throw new MapApiError(error.status, error.response, payload.code as MapErrorCode);
      }
    }
    throw error;
  }
}

function projectRoot(organizationId: string, projectId: string): string {
  return projectApiPath(organizationId, projectId);
}

function mapRoot(organizationId: string, projectId: string): string {
  return projectRoot(organizationId, projectId) + "/map";
}

function layerRoot(organizationId: string, projectId: string, layerId: string): string {
  return projectRoot(organizationId, projectId) + "/layers/" + encodeURIComponent(layerId);
}

function pinParams(options: MapDisplayExtentOptions): URLSearchParams {
  return new URLSearchParams({
    content_revision: options.content_revision, schema_revision: options.schema_revision
  });
}

/** Reads the current authorized configuration, including a virtual unsaved revision zero. */
export function getMap(client: ApiClient, organizationId: string, projectId: string): Promise<ProjectMap> {
  return mapRequest(client, mapRoot(organizationId, projectId));
}

/** Explicit save only. The caller retains this exact operation and intent for ambiguous outcomes. */
export function saveMap(
  client: ApiClient, organizationId: string, projectId: string, input: SaveMapInput
): Promise<MapSaveReceipt> {
  return mapRequest(client, mapRoot(organizationId, projectId), {
    method: "PUT", body: JSON.stringify(input)
  });
}

/** Own-operation receipt requires fresh edit authority; it may precede the current map revision. */
export function getMapOperation(
  client: ApiClient, organizationId: string, projectId: string, operationId: string
): Promise<MapSaveReceipt> {
  return mapRequest(client, mapRoot(organizationId, projectId) +
    "/operations/" + encodeURIComponent(operationId));
}

export function listMapHistory(
  client: ApiClient, organizationId: string, projectId: string, options: MapHistoryOptions = {}
): Promise<MapHistoryPage> {
  const params = new URLSearchParams();
  if (options.limit !== undefined) params.set("limit", String(options.limit));
  if (options.after_revision !== undefined) params.set("after_revision", String(options.after_revision));
  const query = params.toString();
  return mapRequest(client, mapRoot(organizationId, projectId) + "/history" + (query ? "?" + query : ""));
}

/** Bounded display-space snapshot. Incomplete results retain their explicit limitation. */
export function getMapViewport(
  client: ApiClient, organizationId: string, projectId: string, layerId: string,
  options: MapViewportOptions
): Promise<MapViewport> {
  const params = pinParams(options);
  params.set("west", String(options.west));
  params.set("south", String(options.south));
  params.set("east", String(options.east));
  params.set("north", String(options.north));
  if (options.limit !== undefined) params.set("limit", String(options.limit));
  return mapRequest(client, layerRoot(organizationId, projectId, layerId) + "/viewport?" + params.toString());
}

/** Full approved display extent; never derives a full extent from a partial viewport prefix. */
export function getMapDisplayExtent(
  client: ApiClient, organizationId: string, projectId: string, layerId: string,
  options: MapDisplayExtentOptions
): Promise<MapDisplayExtent> {
  return mapRequest(client, layerRoot(organizationId, projectId, layerId) +
    "/display-extent?" + pinParams(options).toString());
}
