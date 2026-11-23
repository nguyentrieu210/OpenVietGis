import { describe, expect, it } from "vitest";

import { ApiClient, ApiError } from "../src/http";
import * as integrationHub from "../src/integration-hub";
import {
  getIntegrationHubAdapterProfile,
  getIntegrationHubCatalog,
  type IntegrationAdapterProfileResponse,
  type IntegrationCatalogResponse
} from "../src/integration-hub";

interface CapturedCall {
  url: string;
  init: RequestInit;
}

function createMockClient(
  payload: unknown,
  status = 200
): { client: ApiClient; calls: CapturedCall[] } {
  const calls: CapturedCall[] = [];
  const client = new ApiClient({
    baseUrl: "http://example.test/api/v1",
    fetchImpl: (async (input: RequestInfo | URL, init?: RequestInit) => {
      calls.push({ url: String(input), init: init ?? {} });
      return new Response(JSON.stringify(payload), {
        status,
        headers: { "content-type": "application/json" }
      });
    }) as typeof fetch
  });
  client.setCsrfToken("csrf-should-not-be-sent-on-get");
  return { client, calls };
}

describe("Integration Hub read-only client SDK", () => {
  it("fetches project catalog via GET with encoded path parameters and preserves truthful policy fields", async () => {
    const fixture: IntegrationCatalogResponse = {
      schema_version: 1,
      organization_id: "org/alpha 1",
      project_id: "proj/beta?x=1",
      mode: "catalog_read_only",
      default_write_policy: "deny",
      scoped_access: true,
      adapters: [
        {
          key: "qgis",
          name: "QGIS",
          category: "gis",
          mode: "open_protocols",
          lifecycle: "planned",
          protocols: ["GeoPackage", "PostGIS", "WMS", "WFS"],
          note: "QGIS plugin and push/pull synchronization not integrated.",
          existing_api: null,
          actions: [
            {
              action: "catalog.discover",
              required_permission: "project.read",
              eligible_by_role: true,
              executable_in_hub: true,
              allowed: true,
              reason: "catalog_only"
            },
            {
              action: "sync.push",
              required_permission: "project.edit",
              eligible_by_role: false,
              executable_in_hub: false,
              allowed: false,
              reason: "permission_denied"
            }
          ]
        }
      ]
    };

    const { client, calls } = createMockClient(fixture);
    const result = await getIntegrationHubCatalog(
      client,
      "org/alpha 1",
      "proj/beta?x=1"
    );

    expect(calls).toHaveLength(1);
    const call = calls[0];
    expect(call?.url).toBe(
      "http://example.test/api/v1/organizations/org%2Falpha%201/projects/proj%2Fbeta%3Fx%3D1/integration-hub/catalog"
    );
    expect(call?.init.method).toBe("GET");
    expect(call?.init.body).toBeUndefined();
    expect(call?.init.credentials).toBe("include");
    expect(new Headers(call?.init.headers).has("x-openvietgis-csrf")).toBe(false);

    expect(result).toEqual(fixture);
    expect(result.mode).toBe("catalog_read_only");
    expect(result.default_write_policy).toBe("deny");
    expect(result.scoped_access).toBe(true);
    expect(result.adapters[0]?.actions[1]?.allowed).toBe(false);
  });

  it("fetches adapter read-only profile via GET with encoded adapterKey and preserves profile and layer metadata", async () => {
    const fixture: IntegrationAdapterProfileResponse = {
      schema_version: 1,
      organization_id: "org-1",
      project_id: "proj-2",
      scoped_access: false,
      default_write_policy: "deny",
      execution_status: "read_only_profile_ready",
      adapter: {
        key: "qgis",
        name: "QGIS",
        category: "gis",
        mode: "open_protocols",
        lifecycle: "planned",
        protocols: ["GeoPackage", "PostGIS", "WMS", "WFS"],
        note: "Read-only file and API exchange profile only; direct desktop sync is not implemented.",
        existing_api: null,
        actions: [
          {
            action: "catalog.discover",
            required_permission: "project.read",
            eligible_by_role: true,
            executable_in_hub: true,
            allowed: true,
            reason: "catalog_only"
          },
          {
            action: "connection.inspect",
            required_permission: "project.read",
            eligible_by_role: true,
            executable_in_hub: false,
            allowed: false,
            reason: "not_implemented"
          },
          {
            action: "sync.push",
            required_permission: "project.edit",
            eligible_by_role: true,
            executable_in_hub: false,
            allowed: false,
            reason: "unsupported_by_adapter"
          }
        ]
      },
      connection_profile: {
        adapter_key: "qgis",
        access_mode: "read_only_exchange",
        supports_direct_db_credentials: false,
        requires_per_layer_acl: true,
        default_write_policy: "deny",
        supported_protocols: ["GeoPackage", "GeoJSON", "WMS", "WFS"],
        existing_endpoints: [
          {
            name: "list_project_layers",
            method: "GET",
            path_template: "/api/v1/organizations/{organization_id}/projects/{project_id}/layers",
            required_permission: "project.read"
          }
        ],
        crs_policy: "explicit_only_never_inferred",
        restrictions: [
          "No direct database credentials are issued.",
          "Per-layer GIS ACL applies to every layer read and export."
        ]
      },
      layers: [
        {
          layer_id: "00000000-0000-4000-8000-000000000101",
          source_id: "00000000-0000-4000-8000-000000000201",
          name: "Thua dat VN2000",
          geometry_family: "Polygon",
          dimensions: "XY",
          validation_state: "ready",
          source_crs_code: 5899,
          stored_crs_code: 5899,
          crs_explicit: true,
          layer_revision: 3,
          content_revision: "a".repeat(64),
          schema_revision: "b".repeat(64),
          export_permitted: true
        },
        {
          layer_id: "00000000-0000-4000-8000-000000000102",
          source_id: "00000000-0000-4000-8000-000000000202",
          name: "Lop chua xac dinh CRS",
          geometry_family: "LineString",
          dimensions: "XY",
          validation_state: "not_ready",
          source_crs_code: null,
          stored_crs_code: null,
          crs_explicit: false,
          layer_revision: 1,
          content_revision: "c".repeat(64),
          schema_revision: "d".repeat(64),
          export_permitted: false
        }
      ]
    };

    const { client, calls } = createMockClient(fixture);
    const result = await getIntegrationHubAdapterProfile(
      client,
      "org/1",
      "proj#2",
      "qgis/custom?v=1"
    );

    expect(calls).toHaveLength(1);
    const call = calls[0];
    expect(call?.url).toBe(
      "http://example.test/api/v1/organizations/org%2F1/projects/proj%232/integration-hub/adapters/qgis%2Fcustom%3Fv%3D1"
    );
    expect(call?.init.method).toBe("GET");
    expect(call?.init.body).toBeUndefined();
    expect(new Headers(call?.init.headers).has("x-openvietgis-csrf")).toBe(false);

    expect(result).toEqual(fixture);
    expect(result.connection_profile?.supports_direct_db_credentials).toBe(false);
    expect(result.connection_profile?.crs_policy).toBe("explicit_only_never_inferred");
    expect(result.layers[0]?.stored_crs_code).toBe(5899);
    expect(result.layers[1]?.crs_explicit).toBe(false);
    expect(result.layers[1]?.stored_crs_code).toBeNull();
  });

  it("exports strictly read-only functions with no write or sync helpers", () => {
    const exportedKeys = Object.keys(integrationHub).sort();
    expect(exportedKeys).toEqual([
      "getIntegrationHubAdapterProfile",
      "getIntegrationHubCatalog"
    ]);
  });

  it("propagates ApiError on forbidden or missing adapter profile", async () => {
    const { client } = createMockClient({ detail: "Not found" }, 404);
    await expect(
      getIntegrationHubAdapterProfile(client, "org-1", "proj-1", "unknown_adapter")
    ).rejects.toBeInstanceOf(ApiError);
  });
});
