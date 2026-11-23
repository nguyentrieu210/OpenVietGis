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
      lifecycle: "planned",
      protocols: ["GeoPackage", "PostGIS", "WMS", "WFS"],
      note: "Read-only profile and layer export only; no direct desktop sync.",
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
          reason: "permission_denied"
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
    existing_endpoints: [
      {
        name: "list_project_layers",
        method: "GET",
        path_template:
          "/api/v1/organizations/{organization_id}/projects/{project_id}/layers",
        required_permission: "project.read"
      }
    ],
    crs_policy: "explicit_only_never_inferred",
    restrictions: [
      "Direct database credentials are never issued.",
      "Per-layer GIS ACL is enforced on every read and export."
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
    "downloadIntegrationLayerExport",
    "getIntegrationHubAdapterProfile",
    "getIntegrationHubCatalog"
  ]);

  const indexSource = await readFile(sdkIndexPath, "utf8");
  assert.match(
    indexSource,
    /export \* from ["']\.\/integration-hub["'];/,
    "client-sdk index must re-export ./integration-hub"
  );

  const requests = [];
  const rawRequests = [];
  const sampleSha = "e".repeat(64);
  const sampleContentRev = "f".repeat(64);
  const sampleSchemaRev = "1".repeat(64);
  const gpkgBytes = new Uint8Array([0x53, 0x51, 0x4c, 0x69, 0x74, 0x65]);

  const mockClient = {
    async request(requestPath, init) {
      requests.push({ path: requestPath, init: init ?? {} });
      return requestPath.includes("/adapters/")
        ? QGIS_PROFILE_FIXTURE
        : CATALOG_FIXTURE;
    },
    async requestRaw(requestPath, init) {
      rawRequests.push({ path: requestPath, init: init ?? {} });
      if (requestPath.includes("forbidden-layer")) {
        const err = new Error("OpenVietGIS API request failed: 403");
        err.name = "ApiError";
        err.status = 403;
        throw err;
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
    mockClient,
    "org/a b",
    "proj/c?x=1"
  );
  assert.equal(
    requests[0].path,
    "/organizations/org%2Fa%20b/projects/proj%2Fc%3Fx%3D1/integration-hub/catalog"
  );
  assert.equal(requests[0].init.method, "GET");
  assert.equal(requests[0].init.body, undefined);
  assert.equal(catalog.mode, "catalog_read_only");
  assert.equal(catalog.default_write_policy, "deny");
  assert.equal(catalog.scoped_access, true);

  const profile = await integrationHub.getIntegrationHubAdapterProfile(
    mockClient,
    "org/a b",
    "proj/c?x=1",
    "qgis/profile#1"
  );
  assert.equal(
    requests[1].path,
    "/organizations/org%2Fa%20b/projects/proj%2Fc%3Fx%3D1/integration-hub/adapters/qgis%2Fprofile%231"
  );
  assert.equal(requests[1].init.method, "GET");
  assert.equal(requests[1].init.body, undefined);
  assert.equal(profile.execution_status, "read_only_profile_ready");
  assert.equal(profile.connection_profile.supports_direct_db_credentials, false);
  assert.equal(profile.connection_profile.crs_policy, "explicit_only_never_inferred");
  assert.equal(profile.layers[0].stored_crs_code, 5899);
  assert.equal(profile.layers[1].crs_explicit, false);
  assert.equal(profile.layers[1].stored_crs_code, null);

  assert.equal(
    integrationHub.buildIntegrationLayerExportPath(
      "org/a b",
      "proj/c?x=1",
      "qgis",
      "layer/id#1"
    ),
    "/organizations/org%2Fa%20b/projects/proj%2Fc%3Fx%3D1/integration-hub/adapters/qgis/layers/layer%2Fid%231/export?format=geopackage"
  );
  assert.equal(
    integrationHub.buildIntegrationLayerExportPath(
      "org-1",
      "proj-1",
      "qgis",
      "layer-4326",
      "geojson"
    ),
    "/organizations/org-1/projects/proj-1/integration-hub/adapters/qgis/layers/layer-4326/export?format=geojson"
  );
  assert.equal(
    integrationHub.buildIntegrationLayerExportPath(
      "org-1",
      "proj-1",
      "postgis",
      "layer-5899",
      "postgis_sql"
    ),
    "/organizations/org-1/projects/proj-1/integration-hub/adapters/postgis/layers/layer-5899/export?format=postgis_sql"
  );

  const qgisExport = await integrationHub.downloadIntegrationLayerExport(
    mockClient,
    "org-1",
    "proj-1",
    "qgis",
    "00000000-0000-4000-8000-000000000101",
    "geopackage"
  );
  assert.equal(
    rawRequests[0].path,
    "/organizations/org-1/projects/proj-1/integration-hub/adapters/qgis/layers/00000000-0000-4000-8000-000000000101/export?format=geopackage"
  );
  assert.equal(rawRequests[0].init.method, "GET");
  assert.equal(rawRequests[0].init.cache, "no-store");
  assert.equal(rawRequests[0].init.body, undefined);
  assert.equal(qgisExport.filename, "ranh-gioi-vn2000.gpkg");
  assert.equal(qgisExport.contentType, "application/geopackage+sqlite3");
  assert.equal(qgisExport.sha256, sampleSha);
  assert.equal(qgisExport.crs, "EPSG:5899");
  assert.equal(qgisExport.contentRevision, sampleContentRev);
  assert.equal(qgisExport.schemaRevision, sampleSchemaRev);
  assert.equal(qgisExport.featureCount, 15);
  assert.deepEqual(
    new Uint8Array(await qgisExport.blob.arrayBuffer()),
    gpkgBytes
  );

  const postgisExport = await integrationHub.downloadIntegrationLayerExport(
    mockClient,
    "org-1",
    "proj-1",
    "postgis",
    "layer-202",
    "postgis_sql"
  );
  assert.equal(
    rawRequests[1].path,
    "/organizations/org-1/projects/proj-1/integration-hub/adapters/postgis/layers/layer-202/export?format=postgis_sql"
  );
  assert.equal(rawRequests[1].init.method, "GET");
  assert.equal(postgisExport.filename, "postgis-layer-202.sql");
  assert.equal(postgisExport.featureCount, 7);
  assert.equal(await postgisExport.blob.text(), "BEGIN;\nSELECT 1;\nCOMMIT;\n");

  await assert.rejects(
    integrationHub.downloadIntegrationLayerExport(
      mockClient,
      "org-1",
      "proj-1",
      "postgis",
      "forbidden-layer",
      "geopackage"
    ),
    (err) => err?.status === 403
  );

  console.log(
    "PASS typed SDK exports, URL encoding, and read-only GET export contract"
  );
}

async function verifyUiAndScopeInvariants() {
  const componentSource = await readFile(componentPath, "utf8");
  const stylesheetSource = await readFile(stylesheetPath, "utf8");
  const sdkSource = await readFile(sdkHubPath, "utf8");
  const sdkTestSource = await readFile(sdkTestPath, "utf8");

  assert.match(componentSource, /getIntegrationHubCatalog/);
  assert.match(componentSource, /getIntegrationHubAdapterProfile/);
  assert.match(componentSource, /downloadIntegrationLayerExport/);
  assert.doesNotMatch(
    componentSource,
    /\/integration-hub\/catalog/,
    "IntegrationCatalog.tsx must use typed SDK helpers instead of raw paths"
  );
  assert.doesNotMatch(
    componentSource,
    /\/integration-hub\/adapters\//,
    "IntegrationCatalog.tsx must use typed SDK export helper instead of raw paths"
  );

  for (const requiredText of [
    "Có dịch vụ chỉ đọc",
    "Đang thử nghiệm",
    "Chưa triển khai",
    "Xem hồ sơ kết nối",
    "Ẩn hồ sơ kết nối",
    "Chưa xác định CRS",
    "Xuất chỉ đọc",
    "Tải .gpkg",
    "Tải .geojson",
    "Tải .sql",
    "Bị hạn chế / chưa đủ CRS",
    "read_only_export",
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
    buttonMatches.length,
    5,
    "IntegrationCatalog.tsx must only contain the inspect toggle and read-only layer download buttons"
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
    /method\s*:\s*["'](POST|PUT|PATCH|DELETE)["']/i
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
      `IntegrationCatalog.tsx must not expose write/sync/credential control: ${forbiddenUiAction}`
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

  console.log("PASS UI read-only invariants, honest labels, and scope hygiene");
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
      if (pathname.includes("/integration-hub/adapters/qgis/layers/") && pathname.endsWith("/export")) {
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

    const pluginButton = page.getByRole("button", { name: /Plugin/i }).first();
    await pluginButton.waitFor({ timeout: 15000 });
    await pluginButton.click();

    const hubSummary = page.locator(".integration-catalog summary");
    await hubSummary.waitFor({ timeout: 10000 });
    await hubSummary.click();

    const qgisCard = page.locator('[data-adapter-key="qgis"]');
    await qgisCard.waitFor({ timeout: 10000 });
    await qgisCard.getByRole("button", { name: "Xem hồ sơ kết nối" }).click();

    await qgisCard.getByText("EPSG:5899").waitFor({ timeout: 10000 });
    await qgisCard.getByText("Chưa xác định CRS").waitFor({ timeout: 10000 });
    await qgisCard.getByText("Không hỗ trợ (false)").waitFor({ timeout: 10000 });
    await qgisCard.getByText("Bị hạn chế / chưa đủ CRS").waitFor({ timeout: 10000 });

    await qgisCard.getByRole("button", { name: "Tải .gpkg" }).click();
    await qgisCard.getByText(/ranh-gioi-vn2000\.gpkg/).waitFor({ timeout: 10000 });

    assert.ok(
      seenMethods.length >= 3 && seenMethods.every((m) => m.method === "GET"),
      "Browser Integration Hub traffic must be strictly GET"
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
