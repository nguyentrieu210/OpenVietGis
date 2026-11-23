import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";

const here = path.dirname(fileURLToPath(import.meta.url));
const repoRoot = path.resolve(here, "../..");

const sdkHubPath = path.join(
  repoRoot,
  "packages/client-sdk/src/integration-hub.ts"
);
const sdkIndexPath = path.join(repoRoot, "packages/client-sdk/src/index.ts");
const sdkTestPath = path.join(
  repoRoot,
  "packages/client-sdk/tests/integration-hub.test.ts"
);
const componentPath = path.join(here, "src/IntegrationCatalog.tsx");
const stylesheetPath = path.join(here, "src/integration-catalog.css");
const bindingManagerPath = path.join(here, "src/QgisBindingManager.tsx");

const CATALOG_FIXTURE = {
  schema_version: 1,
  organization_id: "org-fixture-1",
  project_id: "proj-fixture-1",
  mode: "catalog_read_only",
  default_write_policy: "deny",
  scoped_access: true,
  adapters: [
    {
      key: "ogc_services",
      name: "OGC WMS/WFS",
      category: "gis",
      mode: "remote_service",
      lifecycle: "existing_read_only",
      protocols: ["WMS", "WFS", "COG", "XYZ"],
      note: "Allowlisted read-only external services only.",
      existing_api: "external-sources",
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
    },
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
          reason: "read_only_export"
        },
        {
          action: "sync.push",
          required_permission: "project.edit",
          eligible_by_role: false,
          executable_in_hub: true,
          allowed: false,
          reason: "permission_denied"
        }
      ]
    },
    {
      key: "autocad",
      name: "AutoCAD / Map 3D",
      category: "cad",
      mode: "file_or_desktop_bridge",
      lifecycle: "existing_read_only",
      protocols: ["DXF"],
      note: "Read-only projected metric DXF export; geometry only.",
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
          eligible_by_role: false,
          executable_in_hub: false,
          allowed: false,
          reason: "unsupported_by_adapter"
        }
      ]
    }
  ]
};

const QGIS_PROFILE_FIXTURE = {
  schema_version: 1,
  organization_id: "org-fixture-1",
  project_id: "proj-fixture-1",
  scoped_access: true,
  default_write_policy: "deny",
  execution_status: "read_only_profile_ready",
  adapter: CATALOG_FIXTURE.adapters[1],
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
      "Direct database credentials are never issued.",
      "Per-layer GIS ACL is enforced on every read and export.",
      "Deletions are blocked by default during two-step GeoPackage sync."
    ]
  },
  layers: [
    {
      layer_id: "00000000-0000-4000-8000-000000000101",
      source_id: "00000000-0000-4000-8000-000000000201",
      name: "Ranh gioi VN2000",
      geometry_family: "Polygon",
      dimensions: "XY",
      validation_state: "ready",
      source_crs_code: 5899,
      stored_crs_code: 5899,
      crs_explicit: true,
      layer_revision: 2,
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

async function verifyTypedSdkRuntime() {
  const integrationHub = await import(pathToFileURL(sdkHubPath).href);

  const exportedNames = Object.keys(integrationHub).sort();
  assert.deepEqual(exportedNames, [
    "buildIntegrationLayerExportPath",
    "changeQgisBindingDraftLifecycle",
    "createQgisBindingDraft",
    "downloadIntegrationLayerExport",
    "getIntegrationHubAdapterProfile",
    "getIntegrationHubCatalog",
    "listIntegrationBindingDrafts",
    "listQgisBindingCandidates",
    "previewQgisGpkgSync",
    "pushQgisGpkgSync"
  ]);

  const indexSource = await readFile(sdkIndexPath, "utf8");
  assert.match(
    indexSource,
    /export \* from ["']\.\/integration-hub["'];/,
    "client-sdk index must re-export ./integration-hub"
  );

  const sampleSha = "e".repeat(64);
  const sampleContentRev = "f".repeat(64);
  const sampleSchemaRev = "1".repeat(64);
  const sampleDiffSha = "2".repeat(64);
  const nextContentRev = "3".repeat(64);
  const gpkgBytes = new Uint8Array([0x53, 0x51, 0x4c, 0x69, 0x74, 0x65]);
  const dxfText = "0\nSECTION\n2\nENTITIES\n0\nENDSEC\n0\nEOF\n";

  const previewResponseFixture = {
    schema_version: 1,
    adapter_key: "qgis",
    layer_id: "00000000-0000-4000-8000-000000000101",
    source_id: "00000000-0000-4000-8000-000000000201",
    stored_crs_code: 5899,
    geometry_family: "Polygon",
    base_content_revision: sampleContentRev,
    current_content_revision: sampleContentRev,
    base_schema_revision: sampleSchemaRev,
    current_schema_revision: sampleSchemaRev,
    stale_revision: false,
    deletions_blocked: true,
    create_count: 1,
    update_count: 2,
    unchanged_count: 12,
    omitted_server_feature_count: 3,
    omitted_server_feature_ids: ["feat-omitted-1", "feat-omitted-2", "feat-omitted-3"],
    diff_sha256: sampleDiffSha,
    can_commit: true,
    operations: [
      {
        op_type: "create",
        feature_id: "feat-1",
        changed_fields: [],
        geometry_changed: true
      }
    ]
  };

  const pushResponseFixture = {
    changeset_id: "00000000-0000-4000-8000-000000000888",
    source_id: "00000000-0000-4000-8000-000000000201",
    layer_id: "00000000-0000-4000-8000-000000000101",
    operation_id: "11111111-1111-4111-8111-111111111111",
    resulting_content_revision: nextContentRev,
    resulting_catalog_revision: 3,
    operation_count: 3,
    create_count: 1,
    update_count: 2,
    omitted_server_feature_count: 3,
    deletions_blocked: true,
    diff_sha256: sampleDiffSha,
    audit_event_id: "00000000-0000-4000-8000-000000000777"
  };

  const capturedCalls = [];
  const defaultCsrfToken = "smoke-client-csrf";
  const makeStatusError = (status) => {
    const err = new Error(`OpenVietGIS API request failed: ${status}`);
    err.name = "ApiError";
    err.status = status;
    return err;
  };
  const normalizeHeaders = (init) => {
    const method = (init?.method ?? "GET").toUpperCase();
    const headers = new Headers(init?.headers);
    if (
      init?.body !== undefined &&
      !(init.body instanceof FormData) &&
      !headers.has("content-type")
    ) {
      headers.set("content-type", "application/json");
    }
    if (method === "POST" && !headers.has("x-openvietgis-csrf")) {
      headers.set("x-openvietgis-csrf", defaultCsrfToken);
    }
    return headers;
  };

  const client = {
    async request(requestPath, init) {
      const headers = normalizeHeaders(init);
      const url = `http://127.0.0.1:8000/api/v1${requestPath}`;
      capturedCalls.push({ url, init: { ...(init ?? {}), headers } });
      if (requestPath.endsWith("/integration-hub/catalog")) {
        return CATALOG_FIXTURE;
      }
      if (
        requestPath.includes("/integration-hub/adapters/") &&
        !requestPath.includes("/layers/")
      ) {
        return QGIS_PROFILE_FIXTURE;
      }
      if (
        requestPath.includes("conflict-layer") &&
        requestPath.endsWith("/sync-push")
      ) {
        throw makeStatusError(409);
      }
      if (requestPath.endsWith("/sync-preview")) {
        return previewResponseFixture;
      }
      if (requestPath.endsWith("/sync-push")) {
        return pushResponseFixture;
      }
      throw makeStatusError(404);
    },
    async requestRaw(requestPath, init) {
      const headers = normalizeHeaders(init);
      const url = `http://127.0.0.1:8000/api/v1${requestPath}`;
      capturedCalls.push({ url, init: { ...(init ?? {}), headers } });
      if (requestPath.includes("forbidden-layer")) {
        throw makeStatusError(403);
      }
      if (requestPath.includes("format=postgis_sql")) {
        return new Response("BEGIN;\nSELECT 1;\nCOMMIT;\n", {
          status: 200,
          headers: {
            "content-type": "application/sql; charset=utf-8",
            "x-content-sha256": sampleSha,
            "x-openvietgis-crs": "EPSG:5899",
            "x-openvietgis-feature-count": "7"
          }
        });
      }
      if (requestPath.includes("format=dxf")) {
        return new Response(dxfText, {
          status: 200,
          headers: {
            "content-type": "application/dxf",
            "x-content-sha256": sampleSha,
            "x-openvietgis-crs": "EPSG:5899",
            "x-openvietgis-content-revision": sampleContentRev,
            "x-openvietgis-schema-revision": sampleSchemaRev,
            "x-openvietgis-feature-count": "9",
            "x-openvietgis-attribute-loss": "true"
          }
        });
      }
      return new Response(gpkgBytes, {
        status: 200,
        headers: {
          "content-type": "application/geopackage+sqlite3",
          "content-disposition": 'attachment; filename="ranh-gioi-vn2000.gpkg"',
          "x-content-sha256": sampleSha,
          "x-openvietgis-crs": "EPSG:5899",
          "x-openvietgis-content-revision": sampleContentRev,
          "x-openvietgis-schema-revision": sampleSchemaRev,
          "x-openvietgis-feature-count": "15"
        }
      });
    }
  };

  const catalog = await integrationHub.getIntegrationHubCatalog(
    client,
    "org/a b",
    "proj/c?x=1"
  );
  assert.equal(
    capturedCalls[0].url,
    "http://127.0.0.1:8000/api/v1/organizations/org%2Fa%20b/projects/proj%2Fc%3Fx%3D1/integration-hub/catalog"
  );
  assert.equal(capturedCalls[0].init.method, "GET");
  assert.equal(
    new Headers(capturedCalls[0].init.headers).has("x-openvietgis-csrf"),
    false
  );
  assert.equal(catalog.mode, "catalog_read_only");
  assert.equal(catalog.default_write_policy, "deny");
  assert.equal(catalog.scoped_access, true);

  const profile = await integrationHub.getIntegrationHubAdapterProfile(
    client,
    "org/a b",
    "proj/c?x=1",
    "qgis/profile#1"
  );
  assert.equal(
    capturedCalls[1].url,
    "http://127.0.0.1:8000/api/v1/organizations/org%2Fa%20b/projects/proj%2Fc%3Fx%3D1/integration-hub/adapters/qgis%2Fprofile%231"
  );
  assert.equal(capturedCalls[1].init.method, "GET");
  assert.equal(profile.execution_status, "read_only_profile_ready");
  assert.equal(profile.connection_profile.supports_direct_db_credentials, false);
  assert.equal(profile.connection_profile.sync_push_enabled, true);
  assert.equal(profile.connection_profile.deletions_blocked_by_default, true);
  assert.equal(profile.connection_profile.crs_policy, "explicit_only_never_inferred");

  assert.equal(
    integrationHub.buildIntegrationLayerExportPath(
      "org-1",
      "proj-1",
      "autocad",
      "layer-5899",
      "dxf"
    ),
    "/organizations/org-1/projects/proj-1/integration-hub/adapters/autocad/layers/layer-5899/export?format=dxf"
  );

  const qgisExport = await integrationHub.downloadIntegrationLayerExport(
    client,
    "org-1",
    "proj-1",
    "qgis",
    "00000000-0000-4000-8000-000000000101",
    "geopackage"
  );
  assert.equal(qgisExport.filename, "ranh-gioi-vn2000.gpkg");
  assert.equal(qgisExport.attributeLoss, false);
  assert.equal(qgisExport.featureCount, 15);
  assert.deepEqual(
    new Uint8Array(await qgisExport.blob.arrayBuffer()),
    gpkgBytes
  );

  const dxfExport = await integrationHub.downloadIntegrationLayerExport(
    client,
    "org-1",
    "proj-1",
    "autocad",
    "layer-5899",
    "dxf"
  );
  assert.equal(dxfExport.filename, "autocad-layer-5899.dxf");
  assert.equal(dxfExport.crs, "EPSG:5899");
  assert.equal(dxfExport.featureCount, 9);
  assert.equal(dxfExport.attributeLoss, true);
  assert.equal(await dxfExport.blob.text(), dxfText);

  const postgisExport = await integrationHub.downloadIntegrationLayerExport(
    client,
    "org-1",
    "proj-1",
    "postgis",
    "layer-202",
    "postgis_sql"
  );
  assert.equal(postgisExport.filename, "postgis-layer-202.sql");
  assert.equal(postgisExport.attributeLoss, false);

  await assert.rejects(
    integrationHub.downloadIntegrationLayerExport(
      client,
      "org-1",
      "proj-1",
      "postgis",
      "forbidden-layer",
      "geopackage"
    ),
    (err) => err?.name === "ApiError" && err?.status === 403
  );

  const uploadBlob = new Blob([gpkgBytes], {
    type: "application/geopackage+sqlite3"
  });
  const preview = await integrationHub.previewQgisGpkgSync(
    client,
    "org-1",
    "proj-1",
    "00000000-0000-4000-8000-000000000101",
    {
      file: uploadBlob,
      filename: "ranh-gioi-edited.gpkg",
      baseContentRevision: sampleContentRev,
      baseSchemaRevision: sampleSchemaRev,
      csrfToken: "preview-csrf-token"
    }
  );
  const previewCall = capturedCalls.at(-1);
  assert.equal(previewCall.init.method, "POST");
  assert.ok(previewCall.url.endsWith("/adapters/qgis/layers/00000000-0000-4000-8000-000000000101/sync-preview"));
  const previewHeaders = new Headers(previewCall.init.headers);
  assert.equal(previewHeaders.get("x-openvietgis-csrf"), "preview-csrf-token");
  assert.equal(previewHeaders.has("content-type"), false);
  assert.ok(previewCall.init.body instanceof FormData);
  assert.equal(previewCall.init.body.get("base_content_revision"), sampleContentRev);
  assert.equal(previewCall.init.body.get("base_schema_revision"), sampleSchemaRev);
  assert.equal(preview.can_commit, true);
  assert.equal(preview.omitted_server_feature_count, 3);
  assert.equal(preview.diff_sha256, sampleDiffSha);

  const callsBeforeGuard = capturedCalls.length;
  await assert.rejects(
    integrationHub.pushQgisGpkgSync(
      client,
      "org-1",
      "proj-1",
      "00000000-0000-4000-8000-000000000101",
      {
        file: uploadBlob,
        filename: "ranh-gioi-edited.gpkg",
        baseContentRevision: sampleContentRev,
        baseSchemaRevision: sampleSchemaRev,
        operationId: "field-sync-disallowed-1",
        expectedDiffSha256: sampleDiffSha
      }
    ),
    /field-sync-/
  );
  assert.equal(
    capturedCalls.length,
    callsBeforeGuard,
    "field-sync-* operationId must be rejected client-side before sending HTTP request"
  );

  const pushResult = await integrationHub.pushQgisGpkgSync(
    client,
    "org-1",
    "proj-1",
    "00000000-0000-4000-8000-000000000101",
    {
      file: uploadBlob,
      filename: "ranh-gioi-edited.gpkg",
      baseContentRevision: sampleContentRev,
      baseSchemaRevision: sampleSchemaRev,
      operationId: "11111111-1111-4111-8111-111111111111",
      expectedDiffSha256: sampleDiffSha,
      summary: "Smoke sync push"
    }
  );
  const pushCall = capturedCalls.at(-1);
  assert.equal(pushCall.init.method, "POST");
  assert.ok(pushCall.url.endsWith("/adapters/qgis/layers/00000000-0000-4000-8000-000000000101/sync-push"));
  const pushHeaders = new Headers(pushCall.init.headers);
  assert.equal(pushHeaders.get("x-openvietgis-csrf"), "smoke-client-csrf");
  assert.equal(pushHeaders.has("content-type"), false);
  assert.ok(pushCall.init.body instanceof FormData);
  assert.equal(
    pushCall.init.body.get("operation_id"),
    "11111111-1111-4111-8111-111111111111"
  );
  assert.equal(pushCall.init.body.get("expected_diff_sha256"), sampleDiffSha);
  assert.equal(pushCall.init.body.get("summary"), "Smoke sync push");
  assert.equal(pushResult.changeset_id, pushResponseFixture.changeset_id);
  assert.equal(pushResult.resulting_content_revision, nextContentRev);
  assert.equal(pushResult.operation_count, 3);

  await assert.rejects(
    integrationHub.pushQgisGpkgSync(
      client,
      "org-1",
      "proj-1",
      "conflict-layer",
      {
        file: uploadBlob,
        baseContentRevision: sampleContentRev,
        baseSchemaRevision: sampleSchemaRev,
        operationId: "22222222-2222-4222-8222-222222222222",
        expectedDiffSha256: sampleDiffSha
      }
    ),
    (err) => err?.name === "ApiError" && err?.status === 409
  );

  console.log(
    "PASS typed SDK exports, DXF attributeLoss, and two-step QGIS sync contract"
  );
}

async function verifyUiAndScopeInvariants() {
  const componentSource = await readFile(componentPath, "utf8");
  const stylesheetSource = await readFile(stylesheetPath, "utf8");
  const sdkSource = await readFile(sdkHubPath, "utf8");
  const sdkTestSource = await readFile(sdkTestPath, "utf8");
  const bindingManagerSource = await readFile(bindingManagerPath, "utf8");

  assert.match(componentSource, /QgisBindingManager/);
  assert.match(bindingManagerSource, /listQgisBindingCandidates/);
  assert.match(bindingManagerSource, /listIntegrationBindingDrafts/);
  assert.match(bindingManagerSource, /createQgisBindingDraft/);
  assert.match(bindingManagerSource, /changeQgisBindingDraftLifecycle/);
  assert.match(bindingManagerSource, /Chưa kết nối/);
  assert.match(bindingManagerSource, /Không tự ghi\/xóa/);
  assert.doesNotMatch(bindingManagerSource, /"activate"/);
  assert.doesNotMatch(bindingManagerSource, /password|token_secret|access_key/i);

  assert.match(componentSource, /getIntegrationHubCatalog/);
  assert.match(componentSource, /getIntegrationHubAdapterProfile/);
  assert.match(componentSource, /downloadIntegrationLayerExport/);
  assert.match(componentSource, /loadCsrfToken/);
  assert.match(componentSource, /previewQgisGpkgSync/);
  assert.match(componentSource, /pushQgisGpkgSync/);
  assert.doesNotMatch(
    componentSource,
    /\/integration-hub\/catalog/,
    "IntegrationCatalog.tsx must use typed SDK helpers instead of raw paths"
  );
  assert.doesNotMatch(
    componentSource,
    /\/integration-hub\/adapters\//,
    "IntegrationCatalog.tsx must use typed SDK helpers instead of raw paths"
  );

  for (const requiredText of [
    "Có thể sử dụng",
    "Đang thử nghiệm",
    "Chưa triển khai",
    "Sử dụng kết nối này",
    "BƯỚC 1",
    "BƯỚC 2",
    "BƯỚC 3",
    "Tải dữ liệu về máy",
    "Gửi chỉnh sửa trở lại",
    "Thông số kỹ thuật và phân quyền",
    "Chưa xác định CRS",
    "Xuất chỉ đọc",
    "Tải .gpkg",
    "Tải .geojson",
    "Tải .sql",
    "Tải .dxf",
    "Cần hệ tọa độ phẳng mét",
    "EPSG:5899/3857",
    "Bị hạn chế / chưa đủ CRS",
    "read_only_export",
    "controlled_changeset_sync",
    "Đồng bộ qua Changeset có kiểm soát",
    "Đồng bộ .gpkg (2 bước)",
    'accept=".gpkg"',
    "1. Xem trước thay đổi (Diff)",
    "2. Xác nhận đẩy Changeset",
    "Xóa bị chặn mặc định",
    "can_commit",
    "omitted_server_feature_count",
    "stale_revision",
    "diff_sha256",
    "changeset_id",
    "resulting_content_revision",
    "operation_count",
    "crypto.randomUUID",
    "URL.createObjectURL",
    "URL.revokeObjectURL",
    "default_write_policy",
    "scoped_access",
    "execution_status",
    "supports_direct_db_credentials",
    "crs_policy",
    "validation_state",
    "export_permitted",
    "required_permission",
    "eligible_by_role",
    "executable_in_hub"
  ]) {
    assert.ok(
      componentSource.includes(requiredText),
      `IntegrationCatalog.tsx must render/use ${requiredText}`
    );
  }

  const buttonMatches = componentSource.match(/<button\b[^>]*>/g) ?? [];
  assert.equal(
    buttonMatches.length >= 12,
    true,
    "Guided Integration Hub must retain export/sync controls and include step selection buttons"
  );
  const typeButtonMatches =
    componentSource.match(/<button\b[\s\S]*?type="button"/g) ?? [];
  assert.equal(
    typeButtonMatches.length,
    buttonMatches.length,
    "Every button in IntegrationCatalog.tsx must be type='button'"
  );

  assert.doesNotMatch(componentSource, /<form\b/i);
  assert.doesNotMatch(componentSource, /type=["']password["']/i);
  assert.doesNotMatch(componentSource, /type=["']submit["']/i);
  assert.doesNotMatch(
    componentSource,
    /method\s*:\s*["'](POST|PUT|PATCH|DELETE)["']/i
  );
  assert.doesNotMatch(
    sdkSource,
    /method\s*:\s*["'](PUT|PATCH|DELETE)["']/i,
    "integration-hub.ts must never issue PUT, PATCH, or DELETE requests"
  );

  for (const forbiddenUiAction of [
    "Kết nối ngay",
    "Đồng bộ ngay",
    "Đẩy dữ liệu",
    "Cấu hình kết nối",
    "Mật khẩu CSDL",
    "connection_string",
    "db_password"
  ]) {
    assert.equal(
      componentSource.includes(forbiddenUiAction),
      false,
      `IntegrationCatalog.tsx must not expose fake/credential control: ${forbiddenUiAction}`
    );
  }

  const componentLines = componentSource.split(/\r?\n/);
  const cssLines = stylesheetSource.split(/\r?\n/);
  assert.ok(
    componentLines.length > 80,
    "IntegrationCatalog.tsx must remain cleanly formatted multi-line TSX"
  );
  assert.ok(
    cssLines.length > 40,
    "integration-catalog.css must remain cleanly formatted multi-line CSS"
  );

  const forbiddenMarkers = [
    ["map", "libre"].join(""),
    ["house", "number"].join("-"),
    ["house", "number"].join("_")
  ];

  const ownedFiles = [
    ["integration-hub.ts", sdkSource],
    ["integration-hub.test.ts", sdkTestSource],
    ["IntegrationCatalog.tsx", componentSource],
    ["integration-catalog.css", stylesheetSource]
  ];
  for (const [label, content] of ownedFiles) {
    const lower = content.toLowerCase();
    for (const marker of forbiddenMarkers) {
      assert.equal(
        lower.includes(marker),
        false,
        `${label} must not reference forbidden scope marker`
      );
    }
  }

  console.log(
    "PASS UI Slice 3 two-step sync, AutoCAD DXF hints, honest policy invariants, and scope hygiene"
  );
}

async function runOptionalBrowserSmoke() {
  const targetUrl = process.env.OVG_TEST_URL;
  if (!targetUrl) {
    console.log(
      "SKIP live browser E2E (OVG_TEST_URL not set; contract and UI invariant checks passed)"
    );
    return;
  }

  assert.equal(
    new URL(targetUrl).hostname,
    "127.0.0.1",
    "Loopback fixture URL only"
  );

  const { chromium } = await import("@playwright/test");
  const org = CATALOG_FIXTURE.organization_id;
  const pid = CATALOG_FIXTURE.project_id;
  const project = {
    id: pid,
    organization_id: org,
    name: "Integration Hub Smoke",
    status: "active"
  };

  const browser = await chromium.launch({
    headless: true,
    args: ["--no-sandbox"]
  });

  try {
    const page = await browser.newPage({ viewport: { width: 1280, height: 860 } });
    const errors = [];
    const seenMethods = [];
    page.on("pageerror", (err) => errors.push(err.message));

    await page.route("**/api/v1/**", (route) => {
      const req = route.request();
      const pathname = new URL(req.url()).pathname;
      if (pathname.includes("/integration-hub/")) {
        seenMethods.push({ method: req.method(), pathname });
      }
      if (pathname.endsWith("/me")) {
        return route.fulfill({
          json: { id: "u-1", display_name: "Smoke User", status: "active" }
        });
      }
      if (pathname.endsWith("/auth/providers")) {
        return route.fulfill({ json: [{ id: "keycloak", display_name: "Keycloak" }] });
      }
      if (pathname.endsWith("/auth/csrf")) {
        return route.fulfill({ json: { token: "smoke-csrf" } });
      }
      if (pathname.endsWith("/me/organizations")) {
        return route.fulfill({
          json: [{ id: org, name: "Smoke Org", role: "viewer", status: "active" }]
        });
      }
      if (pathname.endsWith("/projects")) {
        return route.fulfill({ json: [project] });
      }
      if (pathname.endsWith("/projects/" + pid)) {
        return route.fulfill({ json: project });
      }
      if (pathname.endsWith("/access/mine")) {
        return route.fulfill({ json: { restricted: true, rules: [] } });
      }
      if (pathname.endsWith("/effective-permissions")) {
        return route.fulfill({ json: { permissions: ["project.read"] } });
      }
      if (pathname.endsWith("/map")) {
        return route.fulfill({
          json: {
            configuration_revision: 1,
            configuration: { layers: [], basemap: "grid", default_viewpoint: null }
          }
        });
      }
      if (pathname.endsWith("/avatar")) {
        return route.fulfill({ json: { data_url: null } });
      }
      if (
        pathname.endsWith("/layers") ||
        pathname.endsWith("/extensions/activations") ||
        pathname.endsWith("/extensions/manifests") ||
        pathname.endsWith("/extensions/partner-contributions")
      ) {
        return route.fulfill({ json: { items: [], next_after_id: null } });
      }
      if (pathname.endsWith("/integration-hub/catalog")) {
        return route.fulfill({ json: CATALOG_FIXTURE });
      }
      if (pathname.endsWith("/integration-hub/adapters/qgis")) {
        return route.fulfill({ json: QGIS_PROFILE_FIXTURE });
      }
      if (
        pathname.includes("/integration-hub/adapters/qgis/layers/") &&
        pathname.endsWith("/export")
      ) {
        return route.fulfill({
          status: 200,
          headers: {
            "content-type": "application/geopackage+sqlite3",
            "content-disposition": 'attachment; filename="ranh-gioi-vn2000.gpkg"',
            "x-content-sha256": "e".repeat(64),
            "x-openvietgis-crs": "EPSG:5899",
            "x-openvietgis-feature-count": "15"
          },
          body: Buffer.from("SQLite")
        });
      }
      return route.fulfill({ status: 404, json: {} });
    });

    await page.route("https://**/*", (route) => route.abort());
    await page.goto(targetUrl, { waitUntil: "domcontentloaded", timeout: 20000 });

    const hubButton = page.getByRole("button", { name: /Kết nối phần mềm/i }).first();
    await hubButton.waitFor({ timeout: 15000 });
    await hubButton.click();

    const qgisCard = page.locator('[data-adapter-key="qgis"]');
    await qgisCard.waitFor({ timeout: 10000 });
    await qgisCard.getByRole("button", { name: "Sử dụng kết nối này" }).click();
    assert.equal(
      await qgisCard.getByRole("button", { name: "Gửi chỉnh sửa trở lại" }).isDisabled(),
      true,
      "Read-only viewers must not be offered QGIS writeback"
    );
    await qgisCard.getByRole("button", { name: "Tải dữ liệu về máy" }).click();

    await qgisCard.getByText("EPSG:5899").waitFor({ timeout: 10000 });
    await qgisCard.getByText("Chưa xác định CRS").waitFor({ timeout: 10000 });
    await qgisCard.getByText("Bị hạn chế / chưa đủ CRS").waitFor({ timeout: 10000 });

    await qgisCard.getByRole("button", { name: "Tải .gpkg" }).click();
    await qgisCard.getByText(/ranh-gioi-vn2000\.gpkg/).waitFor({ timeout: 10000 });

    assert.ok(
      seenMethods.length >= 3 && seenMethods.every((m) => m.method === "GET"),
      "Viewer browser Integration Hub traffic must be strictly GET"
    );
    assert.deepEqual(errors, []);
    console.log("PASS live browser Integration Hub read-only inspection and export");
  } finally {
    await browser.close();
  }
}

await verifyTypedSdkRuntime();
await verifyUiAndScopeInvariants();
await runOptionalBrowserSmoke();
