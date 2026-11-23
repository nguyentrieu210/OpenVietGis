import { describe, expect, it } from "vitest";

import { ApiClient, ApiError } from "../src/http";
import * as integrationHub from "../src/integration-hub";
import {
  buildIntegrationLayerExportPath,
  downloadIntegrationLayerExport,
  getIntegrationHubAdapterProfile,
  getIntegrationHubCatalog,
  previewQgisGpkgSync,
  pushQgisGpkgSync,
  type ActionReason,
  type IntegrationAdapterProfileResponse,
  type IntegrationCatalogResponse,
  type IntegrationSyncPreviewResponse,
  type IntegrationSyncPushResponse
} from "../src/integration-hub";

interface CapturedCall {
  url: string;
  init: RequestInit;
}

function createMockClient(
  payload: unknown,
  status = 200,
  csrfToken = "csrf-should-not-be-sent-on-get"
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
  client.setCsrfToken(csrfToken);
  return { client, calls };
}

function createBinaryMockClient(
  body: BodyInit,
  options: { status?: number; headers?: Record<string, string> } = {}
): { client: ApiClient; calls: CapturedCall[] } {
  const calls: CapturedCall[] = [];
  const client = new ApiClient({
    baseUrl: "http://example.test/api/v1",
    fetchImpl: (async (input: RequestInfo | URL, init?: RequestInit) => {
      calls.push({ url: String(input), init: init ?? {} });
      return new Response(body, {
        status: options.status ?? 200,
        headers: options.headers
      });
    }) as typeof fetch
  });
  client.setCsrfToken("csrf-should-not-be-sent-on-get");
  return { client, calls };
}

describe("Integration Hub client SDK", () => {
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
          lifecycle: "existing_read_only",
          protocols: ["GeoPackage", "PostGIS", "WMS", "WFS"],
          note: "Read-only export and controlled two-step Changeset sync.",
          existing_api: "integration-hub",
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
              action: "data.export",
              required_permission: "project.read",
              eligible_by_role: true,
              executable_in_hub: true,
              allowed: true,
              reason: "read_only_export" satisfies ActionReason
            },
            {
              action: "sync.push",
              required_permission: "project.edit",
              eligible_by_role: true,
              executable_in_hub: true,
              allowed: true,
              reason: "controlled_changeset_sync" satisfies ActionReason
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
    expect(result.adapters[0]?.actions[1]?.reason).toBe("read_only_export");
    expect(result.adapters[0]?.actions[2]?.reason).toBe(
      "controlled_changeset_sync"
    );
  });

  it("fetches adapter profile via GET with encoded adapterKey and preserves sync policy flags and layer metadata", async () => {
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
        lifecycle: "existing_read_only",
        protocols: ["GeoPackage", "PostGIS", "WMS", "WFS"],
        note: "Read-only export and controlled two-step Changeset sync.",
        existing_api: "integration-hub",
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
            action: "data.export",
            required_permission: "project.read",
            eligible_by_role: true,
            executable_in_hub: true,
            allowed: true,
            reason: "read_only_export"
          },
          {
            action: "sync.push",
            required_permission: "project.edit",
            eligible_by_role: true,
            executable_in_hub: true,
            allowed: true,
            reason: "controlled_changeset_sync"
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
        export_formats: ["geopackage", "geojson"],
        sync_push_enabled: true,
        deletions_blocked_by_default: true,
        existing_endpoints: [
          {
            name: "list_project_layers",
            method: "GET",
            path_template:
              "/api/v1/organizations/{organization_id}/projects/{project_id}/layers",
            required_permission: "project.read"
          },
          {
            name: "qgis_sync_preview",
            method: "POST",
            path_template:
              "/api/v1/organizations/{organization_id}/projects/{project_id}/integration-hub/adapters/qgis/layers/{layer_id}/sync-preview",
            required_permission: "project.edit"
          }
        ],
        crs_policy: "explicit_only_never_inferred",
        restrictions: [
          "No direct database credentials are issued.",
          "Per-layer GIS ACL applies to every layer read and export.",
          "Deletions are blocked by default during two-step GeoPackage sync."
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
    expect(result.connection_profile?.sync_push_enabled).toBe(true);
    expect(result.connection_profile?.deletions_blocked_by_default).toBe(true);
    expect(result.connection_profile?.export_formats).toEqual([
      "geopackage",
      "geojson"
    ]);
    expect(result.connection_profile?.crs_policy).toBe(
      "explicit_only_never_inferred"
    );
    expect(result.layers[0]?.stored_crs_code).toBe(5899);
    expect(result.layers[1]?.crs_explicit).toBe(false);
    expect(result.layers[1]?.stored_crs_code).toBeNull();
  });

  it("builds URL-encoded read-only layer export paths for qgis, postgis, and autocad dxf", () => {
    expect(
      buildIntegrationLayerExportPath(
        "org/a 1",
        "proj/b?x=1",
        "qgis",
        "layer/id#9"
      )
    ).toBe(
      "/organizations/org%2Fa%201/projects/proj%2Fb%3Fx%3D1/integration-hub/adapters/qgis/layers/layer%2Fid%239/export?format=geopackage"
    );

    expect(
      buildIntegrationLayerExportPath(
        "org-1",
        "proj-1",
        "qgis",
        "layer-4326",
        "geojson"
      )
    ).toBe(
      "/organizations/org-1/projects/proj-1/integration-hub/adapters/qgis/layers/layer-4326/export?format=geojson"
    );

    expect(
      buildIntegrationLayerExportPath(
        "org-1",
        "proj-1",
        "postgis",
        "layer-5899",
        "postgis_sql"
      )
    ).toBe(
      "/organizations/org-1/projects/proj-1/integration-hub/adapters/postgis/layers/layer-5899/export?format=postgis_sql"
    );

    expect(
      buildIntegrationLayerExportPath(
        "org-1",
        "proj-1",
        "autocad",
        "layer-5899",
        "dxf"
      )
    ).toBe(
      "/organizations/org-1/projects/proj-1/integration-hub/adapters/autocad/layers/layer-5899/export?format=dxf"
    );
  });

  it("downloads read-only QGIS GeoPackage via GET and parses integrity, CRS, revision, and attributeLoss headers", async () => {
    const bytes = new Uint8Array([0x53, 0x51, 0x4c, 0x69, 0x74, 0x65]);
    const sha = "a".repeat(64);
    const contentRev = "b".repeat(64);
    const schemaRev = "c".repeat(64);

    const { client, calls } = createBinaryMockClient(bytes, {
      status: 200,
      headers: {
        "content-type": "application/geopackage+sqlite3",
        "content-disposition": 'attachment; filename="thua-dat-vn2000.gpkg"',
        "x-content-sha256": sha,
        "x-openvietgis-crs": "EPSG:5899",
        "x-openvietgis-content-revision": contentRev,
        "x-openvietgis-schema-revision": schemaRev,
        "x-openvietgis-feature-count": "42"
      }
    });

    const result = await downloadIntegrationLayerExport(
      client,
      "org/alpha",
      "proj/beta",
      "qgis",
      "layer-101",
      "geopackage"
    );

    expect(calls).toHaveLength(1);
    const call = calls[0];
    expect(call?.url).toBe(
      "http://example.test/api/v1/organizations/org%2Falpha/projects/proj%2Fbeta/integration-hub/adapters/qgis/layers/layer-101/export?format=geopackage"
    );
    expect(call?.init.method).toBe("GET");
    expect(call?.init.cache).toBe("no-store");
    expect(call?.init.body).toBeUndefined();
    expect(call?.init.credentials).toBe("include");
    expect(new Headers(call?.init.headers).has("x-openvietgis-csrf")).toBe(false);

    expect(result.filename).toBe("thua-dat-vn2000.gpkg");
    expect(result.contentType).toBe("application/geopackage+sqlite3");
    expect(result.sha256).toBe(sha);
    expect(result.crs).toBe("EPSG:5899");
    expect(result.contentRevision).toBe(contentRev);
    expect(result.schemaRevision).toBe(schemaRev);
    expect(result.featureCount).toBe(42);
    expect(result.attributeLoss).toBe(false);
    expect(new Uint8Array(await result.blob.arrayBuffer())).toEqual(bytes);
  });

  it("downloads read-only AutoCAD DXF via GET, exposes attributeLoss=true, and uses .dxf fallback extension", async () => {
    const dxfPayload = "0\nSECTION\n2\nENTITIES\n0\nENDSEC\n0\nEOF\n";
    const sha = "d".repeat(64);
    const contentRev = "e".repeat(64);
    const schemaRev = "f".repeat(64);

    const { client, calls } = createBinaryMockClient(dxfPayload, {
      status: 200,
      headers: {
        "content-type": "application/dxf",
        "x-content-sha256": sha,
        "x-openvietgis-crs": "EPSG:5899",
        "x-openvietgis-content-revision": contentRev,
        "x-openvietgis-schema-revision": schemaRev,
        "x-openvietgis-feature-count": "12",
        "x-openvietgis-attribute-loss": "true"
      }
    });

    const result = await downloadIntegrationLayerExport(
      client,
      "org-1",
      "proj-1",
      "autocad",
      "layer-5899",
      "dxf"
    );

    expect(calls).toHaveLength(1);
    const call = calls[0];
    expect(call?.url).toBe(
      "http://example.test/api/v1/organizations/org-1/projects/proj-1/integration-hub/adapters/autocad/layers/layer-5899/export?format=dxf"
    );
    expect(call?.init.method).toBe("GET");
    expect(call?.init.cache).toBe("no-store");
    expect(new Headers(call?.init.headers).has("x-openvietgis-csrf")).toBe(false);

    expect(result.filename).toBe("autocad-layer-5899.dxf");
    expect(result.contentType).toBe("application/dxf");
    expect(result.sha256).toBe(sha);
    expect(result.crs).toBe("EPSG:5899");
    expect(result.contentRevision).toBe(contentRev);
    expect(result.schemaRevision).toBe(schemaRev);
    expect(result.featureCount).toBe(12);
    expect(result.attributeLoss).toBe(true);
    expect(await result.blob.text()).toBe(dxfPayload);
  });

  it("handles RFC 5987 Content-Disposition filenames and falls back safely when header is absent or unsafe", async () => {
    const sqlText = "BEGIN;\nSELECT 1;\nCOMMIT;\n";
    const { client: utf8Client } = createBinaryMockClient(sqlText, {
      headers: {
        "content-type": "application/sql; charset=utf-8",
        "content-disposition":
          "attachment; filename*=UTF-8''..%2F..%2Fpostgis-thua-dat.sql",
        "x-openvietgis-crs": "EPSG:5899",
        "x-openvietgis-feature-count": "0"
      }
    });

    const sqlResult = await downloadIntegrationLayerExport(
      utf8Client,
      "org-1",
      "proj-1",
      "postgis",
      "layer-202",
      "postgis_sql"
    );
    expect(sqlResult.filename).toBe("postgis-thua-dat.sql");
    expect(sqlResult.featureCount).toBe(0);
    expect(sqlResult.attributeLoss).toBe(false);
    expect(await sqlResult.blob.text()).toBe(sqlText);

    const { client: fallbackClient } = createBinaryMockClient(
      '{"type":"FeatureCollection","features":[]}',
      {
        headers: {
          "content-type": "application/geo+json"
        }
      }
    );

    const geojsonResult = await downloadIntegrationLayerExport(
      fallbackClient,
      "org-1",
      "proj-1",
      "qgis",
      "layer-303",
      "geojson"
    );
    expect(geojsonResult.filename).toBe("qgis-layer-303.geojson");
    expect(geojsonResult.sha256).toBeNull();
    expect(geojsonResult.crs).toBeNull();
    expect(geojsonResult.contentRevision).toBeNull();
    expect(geojsonResult.schemaRevision).toBeNull();
    expect(geojsonResult.featureCount).toBeNull();
    expect(geojsonResult.attributeLoss).toBe(false);
  });

  it("propagates ApiError on 403 forbidden and 422 CRS or format rejection during export", async () => {
    const { client: forbiddenClient } = createMockClient(
      { detail: "Export denied by per-layer ACL." },
      403
    );
    await expect(
      downloadIntegrationLayerExport(
        forbiddenClient,
        "org-1",
        "proj-1",
        "postgis",
        "layer-secret",
        "postgis_sql"
      )
    ).rejects.toMatchObject({ name: "ApiError", status: 403 });

    const { client: invalidCrsClient } = createMockClient(
      { detail: "GeoJSON export requires explicit EPSG:4326 stored CRS." },
      422
    );
    await expect(
      downloadIntegrationLayerExport(
        invalidCrsClient,
        "org-1",
        "proj-1",
        "qgis",
        "layer-5899",
        "geojson"
      )
    ).rejects.toMatchObject({ name: "ApiError", status: 422 });
  });

  it("posts multipart FormData to QGIS sync-preview without manual Content-Type and attaches CSRF token", async () => {
    const baseContentRev = "a".repeat(64);
    const baseSchemaRev = "b".repeat(64);
    const diffSha = "c".repeat(64);
    const previewFixture: IntegrationSyncPreviewResponse = {
      schema_version: 1,
      adapter_key: "qgis",
      layer_id: "00000000-0000-4000-8000-000000000101",
      source_id: "00000000-0000-4000-8000-000000000201",
      stored_crs_code: 5899,
      geometry_family: "Polygon",
      base_content_revision: baseContentRev,
      current_content_revision: baseContentRev,
      base_schema_revision: baseSchemaRev,
      current_schema_revision: baseSchemaRev,
      stale_revision: false,
      deletions_blocked: true,
      create_count: 1,
      update_count: 2,
      unchanged_count: 10,
      omitted_server_feature_count: 1,
      omitted_server_feature_ids: ["feat-omitted-1"],
      diff_sha256: diffSha,
      can_commit: true,
      operations: [
        {
          op_type: "create",
          feature_id: "feat-new-1",
          changed_fields: [],
          geometry_changed: true
        },
        {
          op_type: "update",
          feature_id: "feat-2",
          changed_fields: ["ten_thua"],
          geometry_changed: false
        }
      ]
    };

    const { client, calls } = createMockClient(
      previewFixture,
      200,
      "client-csrf-token"
    );
    const gpkgBlob = new Blob([new Uint8Array([0x53, 0x51, 0x4c, 0x69])], {
      type: "application/geopackage+sqlite3"
    });

    const result = await previewQgisGpkgSync(
      client,
      "org/alpha",
      "proj/beta",
      "layer/101",
      {
        file: gpkgBlob,
        filename: "edited-parcels.gpkg",
        baseContentRevision: baseContentRev,
        baseSchemaRevision: baseSchemaRev,
        csrfToken: "explicit-csrf-token"
      }
    );

    expect(calls).toHaveLength(1);
    const call = calls[0];
    expect(call?.url).toBe(
      "http://example.test/api/v1/organizations/org%2Falpha/projects/proj%2Fbeta/integration-hub/adapters/qgis/layers/layer%2F101/sync-preview"
    );
    expect(call?.init.method).toBe("POST");
    expect(call?.init.cache).toBe("no-store");
    const headers = new Headers(call?.init.headers);
    expect(headers.get("x-openvietgis-csrf")).toBe("explicit-csrf-token");
    expect(headers.has("content-type")).toBe(false);

    expect(call?.init.body).toBeInstanceOf(FormData);
    const form = call?.init.body as FormData;
    expect(form.get("base_content_revision")).toBe(baseContentRev);
    expect(form.get("base_schema_revision")).toBe(baseSchemaRev);
    const uploaded = form.get("file");
    expect(uploaded).toBeInstanceOf(Blob);
    expect((uploaded as File).name).toBe("edited-parcels.gpkg");

    expect(result).toEqual(previewFixture);

    // Without explicit csrfToken, ApiClient uses its configured CSRF token.
    await previewQgisGpkgSync(client, "org-1", "proj-1", "layer-101", {
      file: gpkgBlob,
      baseContentRevision: baseContentRev,
      baseSchemaRevision: baseSchemaRev
    });
    const secondHeaders = new Headers(calls[1]?.init.headers);
    expect(secondHeaders.get("x-openvietgis-csrf")).toBe("client-csrf-token");
    expect(secondHeaders.has("content-type")).toBe(false);
  });

  it("posts multipart FormData to QGIS sync-push, enforces field-sync-* client guard, and propagates 409 conflicts", async () => {
    const baseContentRev = "a".repeat(64);
    const baseSchemaRev = "b".repeat(64);
    const diffSha = "c".repeat(64);
    const nextContentRev = "d".repeat(64);

    const pushFixture: IntegrationSyncPushResponse = {
      changeset_id: "00000000-0000-4000-8000-000000000999",
      source_id: "00000000-0000-4000-8000-000000000201",
      layer_id: "00000000-0000-4000-8000-000000000101",
      operation_id: "11111111-1111-4111-8111-111111111111",
      resulting_content_revision: nextContentRev,
      resulting_catalog_revision: 4,
      operation_count: 3,
      create_count: 1,
      update_count: 2,
      omitted_server_feature_count: 0,
      deletions_blocked: true,
      diff_sha256: diffSha,
      audit_event_id: "00000000-0000-4000-8000-000000000777"
    };

    const gpkgBlob = new Blob([new Uint8Array([0x53, 0x51, 0x4c, 0x69])], {
      type: "application/geopackage+sqlite3"
    });

    const { client: guardClient, calls: guardCalls } = createMockClient(
      pushFixture,
      201,
      "csrf-guard"
    );
    await expect(
      pushQgisGpkgSync(guardClient, "org-1", "proj-1", "layer-101", {
        file: gpkgBlob,
        filename: "edited.gpkg",
        baseContentRevision: baseContentRev,
        baseSchemaRevision: baseSchemaRev,
        operationId: "  field-sync-forbidden-123 ",
        expectedDiffSha256: diffSha
      })
    ).rejects.toThrow(/field-sync-/);
    expect(guardCalls).toHaveLength(0);

    const { client, calls } = createMockClient(pushFixture, 201, "csrf-push");
    const pushResult = await pushQgisGpkgSync(
      client,
      "org/alpha",
      "proj/beta",
      "layer/101",
      {
        file: gpkgBlob,
        filename: "edited.gpkg",
        baseContentRevision: baseContentRev,
        baseSchemaRevision: baseSchemaRev,
        operationId: "11111111-1111-4111-8111-111111111111",
        expectedDiffSha256: diffSha,
        summary: "QGIS parcel update",
        csrfToken: "push-csrf-override"
      }
    );

    expect(calls).toHaveLength(1);
    const call = calls[0];
    expect(call?.url).toBe(
      "http://example.test/api/v1/organizations/org%2Falpha/projects/proj%2Fbeta/integration-hub/adapters/qgis/layers/layer%2F101/sync-push"
    );
    expect(call?.init.method).toBe("POST");
    expect(call?.init.cache).toBe("no-store");
    const headers = new Headers(call?.init.headers);
    expect(headers.get("x-openvietgis-csrf")).toBe("push-csrf-override");
    expect(headers.has("content-type")).toBe(false);

    expect(call?.init.body).toBeInstanceOf(FormData);
    const form = call?.init.body as FormData;
    expect(form.get("base_content_revision")).toBe(baseContentRev);
    expect(form.get("base_schema_revision")).toBe(baseSchemaRev);
    expect(form.get("operation_id")).toBe(
      "11111111-1111-4111-8111-111111111111"
    );
    expect(form.get("expected_diff_sha256")).toBe(diffSha);
    expect(form.get("summary")).toBe("QGIS parcel update");
    expect((form.get("file") as File).name).toBe("edited.gpkg");

    expect(pushResult).toEqual(pushFixture);

    const { client: conflictClient } = createMockClient(
      { detail: "Stale base_content_revision or diff_sha256 mismatch." },
      409,
      "csrf-conflict"
    );
    await expect(
      pushQgisGpkgSync(conflictClient, "org-1", "proj-1", "layer-101", {
        file: gpkgBlob,
        filename: "edited.gpkg",
        baseContentRevision: baseContentRev,
        baseSchemaRevision: baseSchemaRev,
        operationId: "22222222-2222-4222-8222-222222222222",
        expectedDiffSha256: diffSha
      })
    ).rejects.toMatchObject({ name: "ApiError", status: 409 });
  });

  it("exports only reviewed read-only and controlled two-step QGIS sync helpers", () => {
    const exportedKeys = Object.keys(integrationHub).sort();
    expect(exportedKeys).toEqual([
      "buildIntegrationLayerExportPath",
      "downloadIntegrationLayerExport",
      "getIntegrationHubAdapterProfile",
      "getIntegrationHubCatalog",
      "previewQgisGpkgSync",
      "pushQgisGpkgSync"
    ]);
  });

  it("propagates ApiError on forbidden or missing adapter profile", async () => {
    const { client } = createMockClient({ detail: "Not found" }, 404);
    await expect(
      getIntegrationHubAdapterProfile(
        client,
        "org-1",
        "proj-1",
        "unknown_adapter"
      )
    ).rejects.toBeInstanceOf(ApiError);
  });
});
