import { expect, test, type Page } from "@playwright/test";
import { signInOwner } from "./frontend-auth-helpers";

// Synthetic OIDC identities and job-local data only; do not retain secret-bearing surfaces.
test.use({ trace: "off", screenshot: "off", video: "off" });

async function signIn(page: Page) {
  await signInOwner(page);
}

async function setup(page: Page, suffix: string) {
  await signIn(page);

  const name = "Spatial team " + suffix;
  await page.goto("/app/organizations");
  await page.getByLabel("Tên tổ chức").fill(name);
  await page.getByLabel("Mã định danh tổ chức").fill("spatial-" + suffix);
  await page.getByRole("button", { name: "Tạo tổ chức", exact: true }).click();

  const organizationLink = page.getByRole("link", { name, exact: true });
  await expect(organizationLink).toBeVisible();
  await organizationLink.click();
  await expect(page).toHaveURL(/\/app\/organizations\/[0-9a-f-]{36}$/);

  const organization = new URL(page.url()).pathname.split("/").at(-1)!;
  expect(organization).toMatch(/^[0-9a-f-]{36}$/);

  await page.goto("/app/organizations/" + organization + "/projects");
  const projectName = "Spatial project " + suffix;
  await page.getByLabel("Tên dự án").fill(projectName);
  await page.getByLabel("Mã định danh dự án").fill("spatial-project-" + suffix);
  await page.getByRole("button", { name: "Tạo dự án", exact: true }).click();

  const row = page.locator('li[data-testid^="project-"]').filter({ hasText: projectName });
  await expect(row).toBeVisible();
  const project = (await row.getAttribute("data-testid"))!.slice("project-".length);
  expect(project).toMatch(/^[0-9a-f-]{36}$/);

  const apiRoot = "/organizations/" + organization + "/projects/" + project;
  const webRoot = "/app" + apiRoot;
  await page.goto(webRoot + "/spatial");
  await expect(
    page.getByRole("heading", { name: "Danh mục nguồn dữ liệu không gian" })
  ).toBeVisible();

  await page.getByLabel("Khóa tham chiếu logic").fill("reference_points_4326");
  await page.getByLabel("Tên nguồn dữ liệu", { exact: true }).fill("Trusted points " + suffix);

  const registration = page.waitForResponse(
    response =>
      new URL(response.url()).pathname === "/api/v1" + apiRoot + "/spatial/sources" &&
      response.request().method() === "POST",
    { timeout: 30_000 }
  );
  await page.getByRole("button", { name: "Đăng ký", exact: true }).click();

  const registered = await registration;
  expect(registered.status()).toBe(201);
  const sourceCreated = await registered.json();
  expect(sourceCreated).toMatchObject({
    organization_id: organization,
    project_id: project,
    resource_type: "data_source",
    lifecycle: "active",
    validation_state: "ready",
    prior_revision: null,
    new_revision: { catalog_revision: 1 }
  });
  expect(sourceCreated.resource_id).toMatch(/^[0-9a-f-]{36}$/);
  expect(sourceCreated.new_revision.content_revision).toMatch(/^[0-9a-f]{64}$/);
  expect(sourceCreated.new_revision.schema_revision).toMatch(/^[0-9a-f]{64}$/);

  const sourceDetail = await page.request.get(
    "/api/v1" + apiRoot + "/spatial/sources/" + sourceCreated.resource_id
  );
  expect(sourceDetail.status()).toBe(200);
  expect(await sourceDetail.json()).toMatchObject({
    id: sourceCreated.resource_id,
    organization_id: organization,
    project_id: project,
    name: "Trusted points " + suffix,
    validation_state: "ready",
    feature_count: 4000,
    stored_crs: { code: 4326 },
    source_crs: { code: 4326 },
    content_revision: sourceCreated.new_revision.content_revision,
    schema_revision: sourceCreated.new_revision.schema_revision
  });

  const sourceButton = page
    .getByRole("list", { name: "Nguồn dữ liệu đã đăng ký" })
    .getByRole("button", { name: "Trusted points " + suffix + " — Sẵn sàng" });
  await expect(sourceButton).toBeVisible();
  await sourceButton.click();

  await page.getByLabel("Tên lớp", { exact: true }).fill("Reference Layer " + suffix);
  const layerCreation = page.waitForResponse(
    response =>
      new URL(response.url()).pathname === "/api/v1" + apiRoot + "/layers" &&
      response.request().method() === "POST",
    { timeout: 30_000 }
  );
  await page.getByRole("button", { name: "Tạo lớp mới", exact: true }).click();

  const createdLayerResponse = await layerCreation;
  expect(createdLayerResponse.status()).toBe(201);
  const layerCreated = await createdLayerResponse.json();
  expect(layerCreated).toMatchObject({
    organization_id: organization,
    project_id: project,
    resource_type: "layer",
    lifecycle: "active",
    prior_revision: null,
    new_revision: { layer_revision: 1 }
  });
  expect(layerCreated.resource_id).toMatch(/^[0-9a-f-]{36}$/);

  const layerDetail = await page.request.get(
    "/api/v1" + apiRoot + "/layers/" + layerCreated.resource_id
  );
  expect(layerDetail.status()).toBe(200);
  expect(await layerDetail.json()).toMatchObject({
    id: layerCreated.resource_id,
    source_id: sourceCreated.resource_id,
    source: {
      id: sourceCreated.resource_id,
      feature_count: 4000,
      validation_state: "ready",
      content_revision: sourceCreated.new_revision.content_revision,
      schema_revision: sourceCreated.new_revision.schema_revision
    }
  });

  const layerButton = page
    .getByRole("list", { name: "Các lớp đã đăng ký" })
    .getByRole("button")
    .filter({ hasText: "Reference Layer " + suffix });
  await expect(layerButton).toBeVisible();
  await layerButton.click();

  await expect(
    page.getByRole("table", { name: "Trang đối tượng theo phiên bản" })
  ).toBeVisible();

  return {
    root: webRoot,
    api: "/api/v1" + apiRoot,
    apiRoot,
    organization,
    project,
    suffix
  };
}

async function mutate(page: Page, path: string, method: "PATCH" | "POST", body: unknown) {
  // Keep the CSRF value entirely inside the browser; it is never a Playwright argument or attachment.
  return page.evaluate(async ({ path, method, body }) => {
    const csrf = await fetch("/api/v1/auth/csrf").then(response => response.json()) as { token: string };
    const response = await fetch(path, { method, headers: { "content-type": "application/json", "x-openvietgis-csrf": csrf.token }, body: JSON.stringify(body) });
    return { status: response.status, body: await response.json() };
  }, { path, method, body });
}

test("registers and inspects stable pinned features, explicit CRS and immutable catalog history", async ({ page }) => {
  test.setTimeout(180_000);
  const data = await setup(page, Date.now().toString());
  const table = page.getByRole("table", { name: "Trang đối tượng theo phiên bản" });
  await table.getByRole("button", { name: "point-000001", exact: true }).click();
  const selection = page.getByRole("region", { name: "Chi tiết đối tượng đã chọn" });
  await expect(selection.getByRole("heading", { name: "Đối tượng point-000001" })).toBeVisible();
  await expect(selection.getByRole("row").filter({ hasText: "fld_measure" })).toContainText('"0"');
  await expect(selection.getByRole("row").filter({ hasText: "fld_note" })).toContainText('""');
  await page.getByRole("button", { name: "Trang đối tượng sau" }).click();
  await expect(table.getByRole("button", { name: "point-000021", exact: true })).toBeVisible();
  await expect(selection.getByRole("heading", { name: "Đối tượng point-000001" })).toBeVisible();
  await page.getByLabel("Hệ tọa độ phản hồi").selectOption("3857");
  await expect(table.getByRole("button", { name: "point-000001", exact: true })).toBeVisible();
  await expect(page.getByRole("region", { name: "Kiểm tra đối tượng" })).toContainText("epsg-1024-forward");
  await expect(selection.getByRole("heading", { name: "Đối tượng point-000001" })).toBeVisible();
  await page.getByLabel("Hệ tọa độ phản hồi").selectOption("4326");
  await expect(table).toBeVisible();
  await page.getByText("Định nghĩa CRS và phép chuyển đổi được phê duyệt", { exact: true }).click();
  const definitions = page.getByRole("table", { name: "Định nghĩa CRS" });
  await expect(definitions).toContainText("EPSG:3405");
  await expect(definitions).toContainText("Chỉ có định nghĩa");
  await expect(page.getByRole("list", { name: "Các phép chuyển đổi trực tiếp đã phê duyệt" }).getByRole("listitem")).toHaveCount(8);
  const sources = await (await page.request.get(data.api + "/spatial/sources")).json();
  const source = sources.items[0];
  expect(source).toMatchObject({ stored_crs: { code: 4326 }, source_crs: { code: 4326 } });
  const renamed = await mutate(page, data.api + "/spatial/sources/" + source.id, "PATCH", { expected_revision: source.catalog_revision, name: "Concurrent rename " + data.suffix });
  expect(renamed.status).toBe(200);
  await page.getByLabel("Tên nguồn mới", { exact: true }).fill("Stale UI rename");
  await page.getByRole("button", { name: "Đổi tên", exact: true }).click();
  await expect(page.getByRole("alert").filter({ hasText: "Nguồn dữ liệu đã thay đổi" })).toBeVisible();
  await expect(page.getByRole("region", { name: "Lịch sử thay đổi dữ liệu không gian" })).toContainText("Cập nhật nguồn");
  const sourceAfter = await (await page.request.get(data.api + "/spatial/sources/" + source.id)).json();
  expect(sourceAfter.content_revision).toBe(source.content_revision);
  expect(sourceAfter.schema_revision).toBe(source.schema_revision);
  expect(await page.evaluate(() => ({ local: Object.keys(localStorage), session: Object.keys(sessionStorage) }))).toEqual({ local: [], session: [] });
  const html = await page.locator("main").innerText();
  expect(html).not.toMatch(/postgresql|spatial\.reference_features|ST_TransformPipeline|urn:ogc/);
});

test("blocks stale feature payloads until explicit retry and rejects a mismatched Project URL", async ({ page }) => {
  test.setTimeout(180_000);
  const data = await setup(page, Date.now().toString() + "-isolation");
  const table = page.getByRole("table", { name: "Trang đối tượng theo phiên bản" });
  await table.getByRole("button", { name: "point-000001", exact: true }).click();
  await expect(page.getByRole("region", { name: "Chi tiết đối tượng đã chọn" })).toBeVisible();
  // The runtime fixture refresh seam is deliberately not public. Inject only the
  // documented HTTP stale response to test the UI; all setup and retry reads are real.
  const featureRoute = "**" + data.api + "/layers/*/features?*";
  await page.route(featureRoute, route => route.fulfill({ status: 409, json: { code: "STALE_READ_PIN", detail: "Read pins changed" } }));
  await page.getByRole("button", { name: "Trang đối tượng sau" }).click();
  await expect(page.getByRole("alert").filter({ hasText: "Nội dung hoặc lược đồ đã thay đổi" })).toBeVisible();
  await expect(page.getByRole("table", { name: "Trang đối tượng theo phiên bản" })).toHaveCount(0);
  await expect(page.getByRole("region", { name: "Chi tiết đối tượng đã chọn" })).toHaveCount(0);
  await page.unroute(featureRoute);
  await page.getByRole("button", { name: "Tải lại phiên bản và bắt đầu lại" }).click();
  await expect(page.getByRole("table", { name: "Trang đối tượng theo phiên bản" }).getByRole("button", { name: "point-000001", exact: true })).toBeVisible();
  await expect(page.getByRole("region", { name: "Chi tiết đối tượng đã chọn" }).getByRole("heading", { name: "Đối tượng point-000001" })).toBeVisible();
  const missing = "00000000-0000-4000-8000-000000000099";
  const badApiRoot = "/organizations/" + data.organization + "/projects/" + missing;
  const badWebRoot = "/app" + badApiRoot;
  for (const path of ["/spatial/sources", "/layers", "/spatial/changes", "/spatial/crs-profiles"]) {
    expect((await page.request.get("/api/v1" + badApiRoot + path)).status()).toBe(404);
  }
  await page.goto(badWebRoot + "/spatial");
  await expect(page.getByRole("alert")).toContainText("Không thể tải quyền truy cập dự án hiện tại");
  await expect(page.getByRole("table", { name: "Trang đối tượng theo phiên bản" })).toHaveCount(0);
  await expect(page.getByRole("region", { name: "Chi tiết đối tượng đã chọn" })).toHaveCount(0);
  await expect(page.getByRole("region", { name: "Lịch sử thay đổi dữ liệu không gian" })).toHaveCount(0);
});

test("removes a warm catalog before a real deleted-Project refetch returns", async ({ page }) => {
  test.setTimeout(180_000);
  const data = await setup(page, Date.now().toString() + "-deleted");
  await page.getByRole("table", { name: "Trang đối tượng theo phiên bản" }).getByRole("button", { name: "point-000001", exact: true }).click();
  await expect(page.getByRole("region", { name: "Chi tiết đối tượng đã chọn" })).toBeVisible();
  await page.getByRole("link", { name: "← Danh sách Dự án", exact: true }).click();
  expect((await mutate(page, data.api, "PATCH", { status: "deleted" })).status).toBe(200);
  let release = () => {};
  const held = new Promise<void>(resolve => { release = resolve; });
  const matches = (url: URL) => url.pathname === data.api;
  await page.route(matches, async route => { await held; await route.continue(); });
  try {
    const requested = page.waitForRequest(request => matches(new URL(request.url())) && request.method() === "GET");
    const denied = page.waitForResponse(response => matches(new URL(response.url())) && response.request().method() === "GET");
    await page.evaluate(() => window.history.back());
    await requested;
    await expect(page.getByRole("list", { name: "Nguồn dữ liệu đã đăng ký" })).toHaveCount(0);
    await expect(page.getByRole("table", { name: "Trang đối tượng theo phiên bản" })).toHaveCount(0);
    await expect(page.getByRole("region", { name: "Lịch sử thay đổi dữ liệu không gian" })).toHaveCount(0);
    release();
    expect((await denied).status()).toBe(404);
    await expect(page.getByRole("alert")).toContainText("Không thể tải quyền truy cập dự án hiện tại");
    await expect(page.getByRole("list", { name: "Nguồn dữ liệu đã đăng ký" })).toHaveCount(0);
    await expect(page.getByRole("region", { name: "Chi tiết đối tượng đã chọn" })).toHaveCount(0);
  } finally {
    release();
    await page.unroute(matches);
  }
});
