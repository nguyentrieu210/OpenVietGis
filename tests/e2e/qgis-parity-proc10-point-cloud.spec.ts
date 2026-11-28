import { readFileSync } from "node:fs";
import { expect, test } from "@playwright/test";
import {
  createMapProject, expectUsableMap, openFixtureMap
} from "./map-fixtures";
import { signInIdentity } from "./frontend-auth-helpers";

test.use({ viewport: { width: 1440, height: 900 }, trace: "off", screenshot: "off", video: "off" });

test("T072 authenticated project previews a bounded real LAZ cloud; anonymous project has no cloud controls", async ({ browser, page }) => {
  test.setTimeout(180_000);
  await signInIdentity(page, "owner-a", "openvietgis-test-owner-a", "/");
  await expect(page.locator(".ovg-account-name")).toContainText("Owner Alpha");
  const fixture = await createMapProject(page, "t072-" + Date.now());
  await openFixtureMap(page, fixture);

  await page.getByRole("button", { name: "Thiết lập 3D" }).click();
  await page.getByRole("button", { name: "3D nâng cao" }).click();
  await page.getByLabel(/Xác nhận CRS ngang/).check();

  const laz = readFileSync(new URL("../../apps/web-next/src/fixtures/point-cloud/ellipsoid-1.4.laz", import.meta.url));
  await page.getByLabel("Mở LAS").setInputFiles({
    name: "ellipsoid-1.4.laz",
    mimeType: "application/octet-stream",
    buffer: laz
  });
  const metadata = page.getByText(/LAZ · ellipsoid-1\.4\.laz · 100\.000\/100\.000 điểm/);
  await expect(metadata).toBeVisible({ timeout: 30_000 });

  // These point-cloud bytes are decoded locally into the current authenticated project
  // session; they are not uploaded or persisted as an asset by this adapter.
  await page.getByRole("button", { name: "Địa cầu" }).click();
  const cloudLayer = page.getByRole("complementary", { name: "Các lớp 3D" });
  await expect(cloudLayer.getByText("ellipsoid-1.4.laz", { exact: true })).toBeVisible();
  await expect(page.locator(".ovg-3d-note")).toContainText("LAZ 100.000 điểm (ellipsoid)");
  const canvas = page.locator('.ovg-cesium-3d[data-ovg3d-engine="cesium"] canvas[aria-label="Bản đồ 3D · điều hướng kiểu Google Earth"]');
  await expect(canvas).toBeVisible({ timeout: 30_000 });

  const anonymousContext = await browser.newContext();
  try {
    const anonymous = await anonymousContext.newPage();
    await anonymous.goto(fixture.root + "/spatial");
    await expect(anonymous.getByRole("link", { name: /Đăng nhập với/ })).toBeVisible();
    await expect(anonymous.getByLabel("Mở LAS")).toHaveCount(0);
    await expect(anonymous.getByRole("button", { name: "3D nâng cao" })).toHaveCount(0);
  } finally {
    await anonymousContext.close();
  }
});
