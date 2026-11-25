import { expect, test } from "@playwright/test";
import { createMapProject, currentMap, mapSignIn } from "./map-fixtures";

test.use({ viewport: { width: 1280, height: 900 }, trace: "off", screenshot: "off", video: "off" });

test("M1 end-to-end: import GeoJSON, view/select/save in Map Workspace, export fixed-revision GeoPackage and download", async ({
  page
}) => {
  test.setTimeout(180_000);
  await mapSignIn(page);
  const fixture = await createMapProject(page, Date.now().toString() + "-m1");

  // 1. Navigate to Import & Export page
  await page.goto(`${fixture.root}/transfers`);
  await expect(page.getByTestId("transfers-page")).toBeVisible();

  // 2. Upload a real RFC 7946 GeoJSON dataset
  const geojson = JSON.stringify({
    type: "FeatureCollection",
    features: [
      {
        type: "Feature",
        id: "m1-pt-01",
        geometry: { type: "Point", coordinates: [106.604, 10.704] },
        properties: { code: "M1-01", name: "Station Alpha", rank: 1, active: true }
      },
      {
        type: "Feature",
        id: "m1-pt-02",
        geometry: { type: "Point", coordinates: [106.606, 10.706] },
        properties: { code: "M1-02", name: "Station Beta", rank: 2, active: false }
      }
    ]
  });
  await page.getByTestId("import-file-input").setInputFiles({
    name: "m1_stations.geojson",
    mimeType: "application/geo+json",
    buffer: Buffer.from(geojson, "utf-8")
  });
  await expect(page.getByTestId("import-inspection")).toBeVisible();
  await expect(page.getByTestId("confirm-import-btn")).toBeEnabled();
  await page.getByTestId("confirm-import-btn").click();
  await expect(page.getByTestId("import-notice")).toContainText("Nhập dữ liệu thành công");

  // 3. Open Map Workspace, add imported Layer, select feature, inspect, save and reload
  await page.goto(`${fixture.root}/map`);
  await expect(page.getByTestId("map-workspace")).toBeVisible();
  await page
    .getByRole("navigation", { name: "Lệnh bản đồ" })
    .getByRole("button", { name: "Thêm dữ liệu", exact: true })
    .click();
  const addDialog = page.getByRole("dialog", { name: "Thêm dữ liệu" });
  const importedCandidate = addDialog.locator("li").filter({ hasText: "m1_stations" });
  await expect(importedCandidate).toContainText("Sẵn sàng");
  await importedCandidate
    .getByRole("button", { name: "Thêm vào bản đồ", exact: true })
    .click();
  await expect(addDialog).not.toBeVisible();
  await page.getByRole("button", { name: "Thu phóng toàn bộ lớp hiển thị", exact: true }).click();
  await expect(page.getByTestId("map-engine-ready")).toHaveAttribute("data-ready", "true");
  await expect(page.getByTestId("map-status")).toHaveAttribute("data-state", "usable");

  await page.getByRole("button", { name: "Chọn m1_stations làm lớp hiện hành", exact: true }).click();
  const table = page.getByRole("table", { name: "Bảng thuộc tính theo phiên bản" });
  await expect(table.getByRole("button", { name: "m1-pt-01", exact: true })).toBeVisible();
  await table.getByRole("button", { name: "m1-pt-01", exact: true }).click();
  await expect(page.getByTestId("map-inspector")).toHaveAttribute("data-feature-id", "m1-pt-01");

  await page.getByRole("button", { name: /^Cấu hình bản đồ dùng chung/ }).click();
  const shared = page.getByRole("region", { name: "Cấu hình bản đồ dùng chung" });
  await shared.getByRole("button", { name: "Lưu bản đồ", exact: true }).click();
  await expect(page.getByTestId("map-save-indicator")).toHaveAttribute("data-save-state", "saved");
  await expect(page.getByTestId("map-save-status")).toContainText("phiên bản bản đồ 1");
  expect((await currentMap(page, fixture)).configuration_revision).toBe(1);

  // 4. Return to Import & Export, preview and confirm lossless GeoPackage export of the fixed revision
  await page.goto(`${fixture.root}/transfers`);
  await expect(page.getByTestId("transfers-page")).toBeVisible();
  const exportLayerSelect = page.getByTestId("export-layer-select");
  const exportLayerId = await exportLayerSelect
    .locator("option")
    .filter({ hasText: "m1_stations" })
    .first()
    .getAttribute("value");
  expect(exportLayerId).toBeTruthy();
  await exportLayerSelect.selectOption(exportLayerId!);
  await page.getByTestId("export-format-select").selectOption("geopackage");
  await page.getByTestId("preview-export-btn").click();
  await expect(page.getByTestId("export-preview")).toBeVisible();
  await page.getByTestId("confirm-export-btn").click();
  await expect(page.getByTestId("export-notice")).toContainText("Xuất dữ liệu thành công");

  // 5. Verify protected artifact download returns real SQLite GeoPackage bytes with SHA-256 header
  const downloadHref = await page
    .locator('[data-testid^="download-artifact-"]')
    .first()
    .getAttribute("href");
  expect(downloadHref).toBeTruthy();
  const dlResponse = await page.request.get(downloadHref!);
  expect(dlResponse.status()).toBe(200);
  expect(dlResponse.headers()["x-content-sha256"]).toMatch(/^[a-f0-9]{64}$/);
  const dlBytes = await dlResponse.body();
  expect(dlBytes.subarray(0, 16).toString("ascii")).toBe("SQLite format 3\0");
});
