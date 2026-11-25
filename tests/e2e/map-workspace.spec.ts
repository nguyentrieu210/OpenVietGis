import { expect, test } from "@playwright/test";
import type { MapHistoryPage, MapSaveReceipt, SaveMapInput } from "../../packages/client-sdk/src";
import {
  createMapProject, currentMap, grantMapCollaborator, mapMutation, mapRead, mapSignIn,
  openFixtureMap, registerMapReferences, saveFixtureMap
} from "./map-fixtures";

// Real local OIDC/API/PostGIS/renderer boundaries. No secret-bearing tracing surfaces.
test.use({ viewport: { width: 1280, height: 900 }, trace: "off", screenshot: "off", video: "off" });

test("opens a real empty grid and arranges, renders, selects and saves catalog Layers without changing data", async ({ page }) => {
  test.setTimeout(240_000);
  await mapSignIn(page);
  const fixture = await createMapProject(page, Date.now().toString() + "-workspace");
  const externalRequests: string[] = [];
  await page.route("**/*", async route => {
    const url = new URL(route.request().url());
    if (["http:", "https:"].includes(url.protocol) && !["127.0.0.1", "localhost"].includes(url.hostname)) {
      externalRequests.push(url.origin); await route.abort(); return;
    }
    await route.continue();
  });
  await page.goto(fixture.root + "/map");
  await expect(page.getByTestId("map-workspace")).toBeVisible();
  await expect(page.getByTestId("map-status")).toHaveAttribute("data-state", "empty");
  await expect(page.getByTestId("map-status")).toHaveText("Không có dữ liệu");
  await expect(page.getByTestId("map-workspace-layout")).toHaveAttribute("data-responsive-mode", "desktop");
  expect((await currentMap(page, fixture)).configuration_revision).toBe(0);

  await registerMapReferences(page, fixture);
  await page.reload();
  const addDataCommand = page
    .getByRole("navigation", { name: "Lệnh bản đồ" })
    .getByRole("button", { name: "Thêm dữ liệu", exact: true });
  for (const layer of fixture.layers) {
    await addDataCommand.click();
    const dialog = page.getByRole("dialog", { name: "Thêm dữ liệu" });
    await expect(dialog).toBeVisible();
    const candidate = dialog.locator("li").filter({ hasText: layer.name });
    await expect(candidate).toContainText("Sẵn sàng");
    await candidate
      .getByRole("button", { name: "Thêm vào bản đồ", exact: true })
      .click();
    await expect(dialog).not.toBeVisible();
    await expect(page.getByTestId("map-layer-" + layer.id)).toBeVisible();
  }

  await addDataCommand.click();
  const duplicateDialog = page.getByRole("dialog", { name: "Thêm dữ liệu" });
  const duplicateCandidate = duplicateDialog
    .locator("li")
    .filter({ hasText: fixture.layers[0]!.name });
  await expect(duplicateCandidate).toContainText("Đã có trên bản đồ");
  await expect(duplicateDialog.getByRole("status")).toContainText("Tất cả lớp dữ liệu hiện có đã được thêm vào bản đồ.");
  await expect(
    duplicateCandidate.getByRole("button", { name: "Đã thêm", exact: true })
  ).toBeDisabled();
  await duplicateDialog
    .getByRole("button", { name: "Đóng hộp Thêm dữ liệu" })
    .click();
  const fittedLayers = new Set<string>();
  const fittedCounts = new Map<string,number>();
  const fittedStarted = performance.now();
  let allResponsesAt: number|undefined;
  const fittedViewport = async (response: import("@playwright/test").Response) => {
    const url = new URL(response.url());
    if (!url.pathname.endsWith("/viewport") || response.request().method() !== "GET" || response.status() !== 200) return;
    const body = await response.json() as {
      complete: boolean;
      items: unknown[];
      context: { layer_id: string };
    };
    if (!body.complete && fixture.layers.some(layer => layer.id === body.context.layer_id)) {
      fittedLayers.add(body.context.layer_id);
      fittedCounts.set(body.context.layer_id,body.items.length);
      if(fittedLayers.size===fixture.layers.length && allResponsesAt===undefined)allResponsesAt=performance.now();
    }
  };
  page.on("response", fittedViewport);
  try {
    await page.getByRole("button", { name: "Thu phóng toàn bộ lớp hiển thị", exact: true }).click();
    await expect.poll(() => fittedLayers.size, { timeout: 30_000 }).toBe(fixture.layers.length);
  } finally {
    page.off("response", fittedViewport);
  }
  expect([...fittedCounts.values()].sort((a,b)=>a-b)).toEqual([500,500,500]);
  const readyStarted=performance.now();
  try {
    await expect(page.getByTestId("map-engine-ready")).toHaveAttribute("data-ready", "true");
  } finally {
    console.log("[R004 fitted view]",JSON.stringify({partialLayers:fittedLayers.size,features:[...fittedCounts.values()],
      responsesMs:Math.round((allResponsesAt??readyStarted)-fittedStarted),
      rendererWaitMs:Math.round(performance.now()-readyStarted),
      totalSinceAllResponsesMs:Math.round(performance.now()-(allResponsesAt??readyStarted)),
      ready:await page.getByTestId("map-engine-ready").getAttribute("data-ready")}));
  }
  await expect(page.getByTestId("map-status")).toHaveAttribute("data-state", "partial", {
    timeout: 15_000
  });
  await expect(page.getByTestId("map-status")).toHaveText("Hiển thị một phần");

  const points = fixture.layers[0]!;
  const pointViewport = page.waitForResponse((response) =>
    new URL(response.url()).pathname.endsWith("/layers/" + points.id + "/viewport") &&
    response.request().method() === "GET"
  );
  await page
    .getByRole("button", { name: "Thu phóng tới " + points.name, exact: true })
    .click();
  expect((await pointViewport).status()).toBe(200);
  await page.getByRole("button", { name: "Chọn Points làm lớp hiện hành", exact: true }).click();
  const table = page.getByRole("table", { name: "Bảng thuộc tính theo phiên bản" });
  await expect(table.getByRole("button", { name: "point-000001", exact: true })).toBeVisible();
  await table.getByRole("button", { name: "point-000001", exact: true }).click();
  await expect(page.getByTestId("map-inspector")).toHaveAttribute("data-feature-id", "point-000001");
  await expect(page.getByTestId("feature-select-point-000001")).toBeChecked();

  // Fit the real selected geometry, then independently pick its rendered center.
  const extentDetail = page.waitForResponse(response =>
    new URL(response.url()).pathname.endsWith("/features/point-000001") && response.request().method() === "GET"
  );
  const selectedViewport = page.waitForResponse(response =>
    new URL(response.url()).pathname === fixture.api + "/layers/" + points.id + "/viewport"
  );
  const command = page.getByRole("textbox", { name: "Nhập lệnh bản đồ" });
  await command.fill("ZS");
  await command.press("Enter");
  await command.blur();
  expect((await extentDetail).status()).toBe(200);
  const selectedFrame = await selectedViewport;
  expect(selectedFrame.status()).toBe(200);
  expect((await selectedFrame.json()).items).toEqual(expect.arrayContaining([
    expect.objectContaining({ feature_id: "point-000001" })
  ]));
  await expect(page.getByTestId("map-engine-ready")).toHaveAttribute("data-ready", "true");
  await page.keyboard.press("Escape");
  await expect(page.getByTestId("map-inspector")).not.toHaveAttribute("data-feature-id", "point-000001");
  const canvas = page.getByTestId("map-canvas");
  await canvas.click();
  await expect(page.getByTestId("map-inspector")).toHaveAttribute("data-feature-id", "point-000001");
  await expect(page.getByTestId("feature-select-point-000001")).toBeChecked();
  await page.getByTestId("feature-select-point-000002").check();
  await expect(page.getByTestId("map-inspector")).toHaveAttribute("data-feature-id", "point-000002");
  await expect(page.getByRole("region", { name: "Bảng thuộc tính bản đồ" })).toContainText("2 đang chọn");
  await page.getByRole("region", { name: "Bảng thuộc tính bản đồ" })
    .getByRole("button", { name: "Trang sau", exact: true }).click();
  await expect(table.getByRole("button", { name: "point-000101", exact: true })).toBeVisible();
  await expect(page.getByTestId("map-inspector")).toHaveAttribute("data-feature-id", "point-000002");
  await expect(page.getByRole("region", { name: "Bảng thuộc tính bản đồ" })).toContainText("2 đang chọn");

  await page.keyboard.press("Escape");
  await page.getByRole("checkbox", { name: "Hiển thị Points", exact: true }).uncheck();
  await canvas.click();
  await expect(page.getByTestId("map-inspector")).not.toHaveAttribute("data-feature-id", "point-000001");
  await page.getByRole("button", { name: "Đưa Polygons lên trên", exact: true }).click();
  await page.getByTestId("map-layer-" + points.id).getByText("Tùy chỉnh hiển thị", { exact: true }).click();
  await page.getByTestId("color-" + points.id).fill("#224466");
  await page.getByRole("button", { name: /^Cấu hình bản đồ dùng chung/ }).click();
  const shared = page.getByRole("region", { name: "Cấu hình bản đồ dùng chung" });
  const savedResponse = page.waitForResponse(response =>
    new URL(response.url()).pathname === fixture.api + "/map" && response.request().method() === "PUT"
  );
  await shared.getByRole("button", { name: "Lưu bản đồ", exact: true }).click();
  expect((await savedResponse).status()).toBe(200);
  await expect(page.getByTestId("map-save-indicator")).toHaveAttribute(
    "data-save-state",
    "saved"
  );
  const persisted = await currentMap(page, fixture);
  expect(persisted.configuration_revision).toBe(1);
  // Incidental fit/zoom/selection never designates a shared startup camera.
  expect(persisted.configuration.default_viewpoint).toBeNull();
  expect(persisted.configuration.layers.find(layer => layer.layer_id === points.id)).toMatchObject({
    visible: false, style: { color: "#224466" }
  });
  await page.reload();
  await expect(page.getByRole("checkbox", { name: "Hiển thị Points", exact: true })).not.toBeChecked();
  await expect(page.getByTestId("color-" + points.id)).toHaveValue("#224466");
  for (const original of fixture.sources) {
    const after = await mapRead<typeof original>(page, fixture.api + "/spatial/sources/" + original.id);
    expect(after).toEqual(original);
  }

  const removable = fixture.layers.find((layer) => layer.id !== points.id)!;
  const originalSource = fixture.sources.find(
    (source) => source.id === removable.source_id
  )!;
  await page
    .getByRole("button", {
      name: "Gỡ " + removable.name + " khỏi bản đồ",
      exact: true
    })
    .click();
  await expect(page.getByTestId("map-layer-" + removable.id)).toHaveCount(0);
  const sourceAfterRemove = await mapRead<typeof originalSource>(
    page,
    fixture.api + "/spatial/sources/" + originalSource.id
  );
  expect(sourceAfterRemove).toEqual(originalSource);

  expect(externalRequests).toEqual([]);
});

test("technical details drawer returns focus and clears on active-layer scope change", async ({ page }) => {
  test.setTimeout(180_000);
  await mapSignIn(page);
  const fixture = await createMapProject(
    page,
    Date.now().toString() + "-technical-drawer"
  );
  await registerMapReferences(page, fixture, true);
  await saveFixtureMap(page, fixture);
  await openFixtureMap(page, fixture);

  const trigger = page.getByRole("button", {
    name: "Thông tin kỹ thuật",
    exact: true
  });
  await trigger.focus();
  await trigger.click();

  const drawer = page.getByRole("dialog", { name: "Thông tin kỹ thuật" });
  await expect(drawer).toBeVisible();
  await expect(
    drawer.getByRole("button", { name: "Đóng thông tin kỹ thuật" })
  ).toBeFocused();

  const current = await currentMap(page, fixture);
  await expect(drawer).toContainText(fixture.organizationId);
  await expect(drawer).toContainText(fixture.projectId);
  await expect(drawer).toContainText(current.map_id);
  const initialLayerId = current.configuration.layers[0]!.layer_id;
  const initialLayer = fixture.layers.find(
    (layer) => layer.id === initialLayerId
  )!;
  await expect(drawer).toContainText(initialLayer.id);
  await expect(drawer).toContainText(initialLayer.source_id);

  await drawer
    .getByRole("button", { name: "Đóng thông tin kỹ thuật" })
    .click();
  await expect(drawer).not.toBeVisible();
  await expect(trigger).toBeFocused();

  const secondLayer = fixture.layers.find(
    (layer) => layer.id !== initialLayer.id
  )!;
  await trigger.click();
  await expect(drawer).toBeVisible();

  // A modal blocks pointer interaction with the workspace, so invoke the same
  // accessible active-layer control programmatically to exercise the scope
  // transition while the drawer is open.
  await page
    .getByRole("button", {
      name: "Chọn " + secondLayer.name + " làm lớp hiện hành",
      exact: true
    })
    .evaluate((element) => (element as HTMLButtonElement).click());

  await expect(drawer).not.toBeVisible();

  await trigger.click();
  await expect(drawer).toBeVisible();
  await expect(drawer).toContainText(secondLayer.id);
  await expect(drawer).toContainText(secondLayer.source_id);
  await expect(drawer).not.toContainText(initialLayer.id);
});

test("preserves the designated camera and reconciles a genuinely committed lost acknowledgement after a later save", async ({ page }) => {
  test.setTimeout(240_000);
  await mapSignIn(page);
  const fixture = await createMapProject(page, Date.now().toString() + "-recovery");
  await registerMapReferences(page, fixture, false);
  await saveFixtureMap(page, fixture);
  await openFixtureMap(page, fixture);
  const original = await currentMap(page, fixture);
  const configuration = page.getByRole("button", { name: /^Cấu hình bản đồ dùng chung/ });
  const shared = page.getByRole("region", { name: "Cấu hình bản đồ dùng chung" });
  await page.keyboard.press("+");
  await page.getByRole("checkbox", { name: "Hiển thị Points", exact: true }).uncheck();
  await configuration.click();
  let capturedIntent: SaveMapInput | null = null;
  const saveUrl = (url: URL) => url.pathname === fixture.api + "/map";
  await page.route(saveUrl, async route => {
    if (route.request().method() !== "PUT") { await route.continue(); return; }
    capturedIntent = route.request().postDataJSON() as SaveMapInput;
    const committed = await route.fetch();
    expect(committed.status()).toBe(200);
    await route.abort("connectionreset");
  });
  await shared.getByRole("button", { name: "Lưu bản đồ", exact: true }).click();
  await expect(page.getByTestId("map-save-indicator")).toHaveAttribute(
    "data-save-state",
    "unresolved"
  );
  await expect(page.getByTestId("map-save-indicator")).toContainText(
    "Chưa xác định kết quả lưu"
  );
  await page.unroute(saveUrl);
  const committed = await currentMap(page, fixture);
  expect(committed.configuration_revision).toBe(2);
  expect(committed.configuration.default_viewpoint).toEqual(original.configuration.default_viewpoint);
  if (!capturedIntent) throw new Error("No actual save operation observed");
  const originalOperation = capturedIntent as SaveMapInput;
  const later = await mapMutation<MapSaveReceipt>(page, fixture.api + "/map", "PUT", {
    ...originalOperation, operation_id: crypto.randomUUID(), expected_revision: 2,
    layers: committed.configuration.layers.map(layer => ({ ...layer, visible: true }))
  });
  expect(later.status).toBe(200);
  expect(later.body!.configuration_revision).toBe(3);
  await shared.getByRole("button", { name: "Đối chiếu bản lưu", exact: true }).click();
  await expect(page.getByTestId("map-save-indicator")).toHaveAttribute(
    "data-save-state",
    "saved"
  );
  const receipt = await mapRead<MapSaveReceipt>(page, fixture.api + "/map/operations/" + originalOperation.operation_id);
  expect(receipt.configuration_revision).toBe(2);
  expect((await currentMap(page, fixture)).configuration_revision).toBe(3);
  const replay = await mapMutation<MapSaveReceipt>(page, fixture.api + "/map", "PUT", originalOperation);
  expect(replay.status).toBe(200);
  expect(replay.body).toEqual(receipt);
  const history = await mapRead<MapHistoryPage>(page, fixture.api + "/map/history");
  expect(history.items.map(item => item.configuration_revision)).toEqual([3, 2, 1]);
  await page.reload();
  await expect(page.getByRole("checkbox", { name: "Hiển thị Points", exact: true })).toBeChecked();
  await configuration.click();
  await expect(page.getByTestId("map-save-indicator")).toHaveAttribute(
    "data-save-state",
    "clean"
  );
  await expect(page.getByTestId("map-save-indicator")).toContainText(
    "Phiên bản bản đồ 3"
  );
});

test("two real actors receive CAS conflicts and revoked access clears a warm rendered selection", async ({ page: owner, browser, baseURL }) => {
  test.setTimeout(300_000);
  await mapSignIn(owner);
  const fixture = await createMapProject(owner, Date.now().toString() + "-actors");
  await registerMapReferences(owner, fixture, false);
  await saveFixtureMap(owner, fixture);
  const context = await browser.newContext({ baseURL, viewport: { width: 1280, height: 900 } });
  try {
    const editor = await context.newPage();
    await mapSignIn(editor, true);
    const grantId = await grantMapCollaborator(owner, editor, fixture, "editor");
    await openFixtureMap(owner, fixture);
    await openFixtureMap(editor, fixture);
    await owner.getByRole("checkbox", { name: "Hiển thị Points", exact: true }).uncheck();
    await owner.getByRole("button", { name: /^Cấu hình bản đồ dùng chung/ }).click();
    await owner.getByRole("region", { name: "Cấu hình bản đồ dùng chung" })
      .getByRole("button", { name: "Lưu bản đồ", exact: true }).click();
    await expect(owner.getByTestId("map-save-indicator")).toHaveAttribute(
      "data-save-state",
      "saved"
    );
    await expect(owner.getByTestId("map-save-indicator")).toContainText(
      "Đã lưu phiên bản bản đồ 2"
    );
    await editor.getByRole("button", { name: /^Cấu hình bản đồ dùng chung/ }).click();
    const shared = editor.getByRole("region", { name: "Cấu hình bản đồ dùng chung" });
    await shared.getByRole("button", { name: "Xóa góc nhìn mặc định", exact: true }).click();
    await shared.getByRole("button", { name: "Lưu bản đồ", exact: true }).click();
    await expect(editor.getByTestId("map-save-indicator")).toHaveAttribute(
      "data-save-state",
      "conflict"
    );
    await expect(editor.getByTestId("map-save-indicator")).toContainText(
      "Có phiên bản mới"
    );
    expect((await currentMap(owner, fixture)).configuration_revision).toBe(2);
    await shared.getByRole("button", { name: "Tải lại bản đồ chung", exact: true }).click();
    await expect(editor.getByTestId("map-save-indicator")).toHaveAttribute(
      "data-save-state",
      "clean"
    );
    await expect(editor.getByTestId("map-save-indicator")).toContainText(
      "Phiên bản bản đồ 2"
    );
    await editor.getByRole("checkbox", { name: "Hiển thị Points", exact: true }).check();
    await editor.getByTestId("feature-select-point-000001").check();
    await expect(editor.getByTestId("map-inspector")).toHaveAttribute("data-feature-id", "point-000001");
    const revoked = await mapMutation(owner,
      "/api/v1/organizations/" + fixture.organizationId + "/grants/" + grantId + "/revoke", "POST");
    expect(revoked.status).toBe(204);
    const denied = editor.waitForResponse(response =>
      new URL(response.url()).pathname === fixture.api + "/map" && response.request().method() === "GET"
    );
    await editor.getByRole("button", { name: "Kiểm tra lại quyền truy cập", exact: true }).click();
    expect((await denied).status()).toBe(403);
    await expect(editor.getByRole("table", { name: "Bảng thuộc tính theo phiên bản" })).toHaveCount(0);
    await expect(editor.getByTestId("map-inspector")).toHaveCount(0);
    const forbidden = await mapMutation(editor, fixture.api + "/map", "PUT", {
      operation_id: crypto.randomUUID(), expected_revision: 2, layers: [], basemap: "grid",
      default_viewpoint: { mode: "preserve" }
    });
    expect(forbidden.status).toBe(403);
    expect((await currentMap(owner, fixture)).configuration_revision).toBe(2);
  } finally {
    await context.close();
  }
});

test("a real viewer can preview locally while direct shared mutation stays denied", async ({ page: owner, browser, baseURL }) => {
  test.setTimeout(240_000);
  await mapSignIn(owner);
  const fixture = await createMapProject(owner, Date.now().toString() + "-viewer");
  await registerMapReferences(owner, fixture, false);
  await saveFixtureMap(owner, fixture);
  const context = await browser.newContext({ baseURL, viewport: { width: 1280, height: 900 } });
  try {
    const viewer = await context.newPage();
    await mapSignIn(viewer, true);
    await grantMapCollaborator(owner, viewer, fixture, "viewer");
    await openFixtureMap(viewer, fixture);
    await viewer.getByRole("checkbox", { name: "Hiển thị Points", exact: true }).uncheck();
    await expect(viewer.getByRole("checkbox", { name: "Hiển thị Points", exact: true })).not.toBeChecked();
    await viewer.getByRole("button", { name: /^Cấu hình bản đồ dùng chung/ }).click();
    await expect(viewer.getByRole("region", { name: "Cấu hình bản đồ dùng chung" })
      .getByRole("button", { name: "Lưu bản đồ", exact: true })).toBeDisabled();
    const attempted = await mapMutation(viewer, fixture.api + "/map", "PUT", {
      operation_id: crypto.randomUUID(), expected_revision: 1, layers: [], basemap: "grid",
      default_viewpoint: { mode: "clear" }
    });
    expect(attempted.status).toBe(403);
    expect((await currentMap(owner, fixture)).configuration_revision).toBe(1);
    await viewer.reload();
    await expect(viewer.getByRole("checkbox", { name: "Hiển thị Points", exact: true })).toBeChecked();
  } finally {
    await context.close();
  }
});

test("keeps camera and selection stable across panel collapse and keyboard resize", async ({ page }) => {
  test.setTimeout(180_000);
  await mapSignIn(page);
  const fixture = await createMapProject(
    page,
    Date.now().toString() + "-layout-selection"
  );
  await registerMapReferences(page, fixture, false);
  await saveFixtureMap(page, fixture);
  await openFixtureMap(page, fixture);

  const points = fixture.layers[0]!;
  await page
    .getByRole("button", {
      name: "Chọn " + points.name + " làm lớp hiện hành",
      exact: true
    })
    .click();

  const table = page.getByRole("table", {
    name: "Bảng thuộc tính theo phiên bản"
  });
  await expect(table.locator("tbody tr")).toHaveCount(100);
  await table
    .getByRole("button", { name: "point-000001", exact: true })
    .click();

  await expect(page.getByTestId("map-inspector")).toHaveAttribute(
    "data-feature-id",
    "point-000001"
  );
  await expect(page.getByTestId("feature-select-point-000001")).toBeChecked();
  await expect(page.getByTestId("accessible-map-summary")).toContainText(
    "1 đối tượng đang chọn"
  );
  await expect(page.getByTestId("accessible-map-summary")).toContainText(
    "Đang xem đối tượng point-000001"
  );

  const cameraBefore = await page
    .getByTestId("map-camera-summary")
    .innerText();

  const controls = page.getByRole("navigation", { name: "Lệnh bản đồ" });
  const layerToggle = page.getByRole("complementary", { name: "Thanh công cụ GIS dọc" })
    .getByRole("button", { name: "Lớp", exact: true });
  const panelToggles = ["Chi tiết", "Bảng thuộc tính"].map(name =>
    controls.getByRole("button", { name, exact: true })
  );
  for (const button of [layerToggle, ...panelToggles]) {
    await button.click();
    await expect(button).toHaveAttribute("aria-pressed", "false");
  }

  await expect(page.getByTestId("map-camera-summary")).toHaveText(cameraBefore);
  await expect(page.getByTestId("map-inspector")).toHaveAttribute(
    "data-feature-id",
    "point-000001"
  );

  await layerToggle.click();
  await expect(layerToggle).toHaveAttribute("aria-pressed", "true");

  const separator = page.getByRole("separator", {
    name: "Đổi độ rộng bảng lớp dữ liệu"
  });
  const widthBefore = Number(await separator.getAttribute("aria-valuenow"));
  await separator.focus();
  await page.keyboard.press("ArrowRight");
  await expect(separator).not.toHaveAttribute(
    "aria-valuenow",
    String(widthBefore)
  );

  for (const button of panelToggles) {
    await button.click();
    await expect(button).toHaveAttribute("aria-pressed", "true");
  }

  await expect(page.getByTestId("map-camera-summary")).toHaveText(cameraBefore);
  await expect(page.getByTestId("map-inspector")).toHaveAttribute(
    "data-feature-id",
    "point-000001"
  );
  await expect(page.getByTestId("feature-select-point-000001")).toBeChecked();
  await expect(page.getByTestId("accessible-map-summary")).toContainText(
    points.name
  );
});

test("warns only for unsaved shared configuration changes", async ({ page }) => {
  test.setTimeout(180_000);
  await mapSignIn(page);
  const fixture = await createMapProject(
    page,
    Date.now().toString() + "-dirty-navigation"
  );
  await registerMapReferences(page, fixture, false);
  await saveFixtureMap(page, fixture);
  await openFixtureMap(page, fixture);

  const indicator = page.getByRole("button", { name: /^Cấu hình bản đồ dùng chung/ });
  await expect(indicator).toHaveAttribute("data-save-state", "clean");

  // Camera, selection and local panel layout are personal/transient state.
  await page.keyboard.press("+");
  await expect(indicator).toHaveAttribute("data-save-state", "clean");

  await page
    .getByRole("button", {
      name: "Chọn Points làm lớp hiện hành",
      exact: true
    })
    .click();
  const table = page.getByRole("table", {
    name: "Bảng thuộc tính theo phiên bản"
  });
  await table
    .getByRole("button", { name: "point-000001", exact: true })
    .click();
  await expect(indicator).toHaveAttribute("data-save-state", "clean");

  const detailToggle = page
    .getByRole("navigation", { name: "Lệnh bản đồ" })
    .getByRole("button", { name: "Chi tiết", exact: true });
  await detailToggle.click();
  await expect(indicator).toHaveAttribute("data-save-state", "clean");
  await detailToggle.click();

  // Shared visibility changes are dirty and must block navigation.
  await page
    .getByRole("checkbox", { name: "Hiển thị Points", exact: true })
    .uncheck();
  await expect(indicator).toHaveAttribute("data-save-state", "dirty");
  await expect(indicator).toContainText("Có thay đổi chưa lưu");

  await page.getByRole("link", { name: "← Dự án", exact: true }).click();
  const blocker = page.getByRole("dialog", {
    name: "Có thay đổi bản đồ chưa lưu"
  });
  await expect(blocker).toBeVisible();
  await blocker.getByRole("button", { name: "Ở lại", exact: true }).click();
  await expect(page).toHaveURL(new RegExp(fixture.root + "/map$"));
  await expect(indicator).toHaveAttribute("data-save-state", "dirty");

  await indicator.click();
  await page
    .getByRole("region", { name: "Cấu hình bản đồ dùng chung" })
    .getByRole("button", { name: "Lưu bản đồ", exact: true })
    .click();
  await expect(indicator).toHaveAttribute("data-save-state", "saved");
  await expect(indicator).toContainText("Đã lưu phiên bản bản đồ 2");

  await page.getByRole("link", { name: "← Dự án", exact: true }).click();
  await expect(page).toHaveURL(
    new RegExp(
      "/app/organizations/" +
        fixture.organizationId +
        "/projects$"
    )
  );
  await expect(
    page.getByRole("dialog", { name: "Có thay đổi bản đồ chưa lưu" })
  ).toHaveCount(0);
});

test("keeps technical identifiers secondary and returns focus after closing the drawer", async ({ page }) => {
  test.setTimeout(180_000);
  await mapSignIn(page);
  const fixture = await createMapProject(
    page,
    Date.now().toString() + "-technical-drawer"
  );
  await registerMapReferences(page, fixture, false);
  await saveFixtureMap(page, fixture);
  const persisted = await currentMap(page, fixture);
  await openFixtureMap(page, fixture);

  const primaryContext = page.getByTestId("gis-ribbon");
  await expect(primaryContext).not.toContainText(fixture.organizationId);
  await expect(primaryContext).not.toContainText(fixture.projectId);
  await expect(primaryContext).not.toContainText(persisted.map_id);

  const trigger = page.getByRole("button", {
    name: "Thông tin kỹ thuật",
    exact: true
  });
  await trigger.click();

  const drawer = page.getByRole("dialog", {
    name: "Thông tin kỹ thuật"
  });
  await expect(drawer).toBeVisible();
  await expect(drawer).toContainText(fixture.organizationId);
  await expect(drawer).toContainText(fixture.projectId);
  await expect(drawer).toContainText(persisted.map_id);
  await expect(drawer).toContainText(fixture.layers[0]!.id);
  await expect(drawer).toContainText(fixture.layers[0]!.source_id);
  await expect(drawer).toContainText(
    fixture.layers[0]!.source.content_revision
  );
  await expect(drawer).toContainText(
    fixture.layers[0]!.source.schema_revision
  );

  await drawer
    .getByRole("button", { name: "Đóng thông tin kỹ thuật" })
    .click();
  await expect(drawer).not.toBeVisible();
  await expect(trigger).toBeFocused();

  await trigger.click();
  await expect(drawer).toBeVisible();
  const otherFixture = await createMapProject(
    page,
    Date.now().toString() + "-technical-scope-change"
  );
  await page.goto(otherFixture.root + "/map");
  await expect(page.getByTestId("map-workspace")).toBeVisible();
  await expect(
    page.getByRole("dialog", { name: "Thông tin kỹ thuật" })
  ).toHaveCount(0);
  await expect(page.getByTestId("gis-ribbon")).not.toContainText(fixture.projectId);
});
