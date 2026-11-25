import { expect, test } from "@playwright/test";
import { visualLaunchOptions } from "./fixtures/visual-launch-options";

import {
  createMapProject,
  mapSignIn,
  openFixtureMap,
  registerMapReferences,
  saveFixtureMap
} from "./map-fixtures";

test.use({
  launchOptions: visualLaunchOptions,
  viewport: { width: 1440, height: 900 },
  trace: "off",
  screenshot: "off",
  video: "off"
});

test("empty GIS workspace visual baseline", async ({ page }) => {
  await mapSignIn(page);
  const fixture = await createMapProject(
    page,
    Date.now().toString() + "-visual-empty"
  );

  await page.goto(fixture.root + "/map");
  await expect(page.getByTestId("map-workspace")).toBeVisible();
  await expect(page.getByTestId("map-status")).toHaveAttribute(
    "data-state",
    "empty"
  );
  await expect(page.getByTestId("map-workspace-layout")).toHaveAttribute(
    "data-responsive-mode",
    "desktop"
  );

  await expect(page.getByTestId("map-workspace")).toHaveScreenshot(
    "map-workspace-empty.png",
    {
      animations: "disabled",
      mask: [
        page.getByTestId("map-canvas"),
        page.getByTestId("gis-ribbon").getByRole("heading", { level: 1 })
      ]
    }
  );
});

test("populated GIS workspace visual baseline", async ({ page }) => {
  await mapSignIn(page);
  const fixture = await createMapProject(
    page,
    Date.now().toString() + "-visual-data"
  );
  await registerMapReferences(page, fixture, false);
  await saveFixtureMap(page, fixture);

  await openFixtureMap(page, fixture);
  await expect(
    page.getByRole("button", {
      name: "Chọn Points làm lớp hiện hành",
      exact: true
    })
  ).toBeVisible();
  await expect(
    page.getByRole("table", { name: "Bảng thuộc tính theo phiên bản" })
  ).toBeVisible();

  await expect(page.getByTestId("map-workspace")).toHaveScreenshot(
    "map-workspace-populated.png",
    {
      animations: "disabled",
      mask: [
        page.getByTestId("map-canvas"),
        page.getByTestId("gis-ribbon").getByRole("heading", { level: 1 })
      ]
    }
  );

});

test("technical drawer closed and open visual baselines", async ({ page }) => {
  await mapSignIn(page);
  const fixture = await createMapProject(
    page,
    Date.now().toString() + "-visual-technical"
  );
  await registerMapReferences(page, fixture, false);
  await saveFixtureMap(page, fixture);
  await openFixtureMap(page, fixture);

  const workspace = page.getByTestId("map-workspace");
  const commonMask = [
    page.getByTestId("map-canvas"),
    page.getByTestId("gis-ribbon").getByRole("heading", { level: 1 })
  ];

  await expect(workspace).toHaveScreenshot(
    "map-workspace-technical-closed.png",
    {
      animations: "disabled",
      mask: commonMask
    }
  );

  await page
    .getByRole("button", { name: "Thông tin kỹ thuật", exact: true })
    .click();
  const drawer = page.getByRole("dialog", { name: "Thông tin kỹ thuật" });
  await expect(drawer).toBeVisible();

  await expect(workspace).toHaveScreenshot(
    "map-workspace-technical-open.png",
    {
      animations: "disabled",
      mask: [
        ...commonMask,
        ...await drawer.locator("code").all()
      ]
    }
  );
});

test("dirty saved and conflict save-state visual baselines", async ({ page }) => {
  test.setTimeout(180_000);
  await mapSignIn(page);
  const fixture = await createMapProject(
    page,
    Date.now().toString() + "-visual-save-state"
  );
  await registerMapReferences(page, fixture, false);
  await saveFixtureMap(page, fixture);
  await openFixtureMap(page, fixture);

  const workspace = page.getByTestId("map-workspace");
  const indicator = page.getByRole("button", { name: /^Cấu hình bản đồ dùng chung/ });
  const screenshotOptions = {
    animations: "disabled" as const,
    mask: [
      page.getByTestId("map-canvas"),
      page.getByTestId("gis-ribbon").getByRole("heading", { level: 1 })
    ]
  };

  await page
    .getByRole("checkbox", { name: "Hiển thị Points", exact: true })
    .uncheck();
  await expect(indicator).toHaveAttribute("data-save-state", "dirty");
  await expect(workspace).toHaveScreenshot(
    "map-workspace-dirty.png",
    screenshotOptions
  );

  await indicator.click();
  await page
    .getByRole("region", { name: "Cấu hình bản đồ dùng chung" })
    .getByRole("button", { name: "Lưu bản đồ", exact: true })
    .click();
  await expect(indicator).toHaveAttribute("data-save-state", "saved");
  await indicator.click();
  await expect(workspace).toHaveScreenshot(
    "map-workspace-saved.png",
    screenshotOptions
  );

  await page
    .getByRole("checkbox", { name: "Hiển thị Points", exact: true })
    .check();
  await expect(indicator).toHaveAttribute("data-save-state", "dirty");

  const savePath = fixture.api + "/map";
  await page.route(savePath, async (route) => {
    if (route.request().method() !== "PUT") {
      await route.continue();
      return;
    }
    await route.fulfill({
      status: 409,
      json: {
        code: "STALE_MAP_REVISION",
        detail: "Synthetic deterministic conflict"
      }
    });
  });

  try {
    await indicator.click();
    await page
      .getByRole("region", { name: "Cấu hình bản đồ dùng chung" })
      .getByRole("button", { name: "Lưu bản đồ", exact: true })
      .click();
    await expect(indicator).toHaveAttribute("data-save-state", "conflict");
    await expect(indicator).toContainText("Có phiên bản mới");
    await expect(workspace).toHaveScreenshot(
      "map-workspace-conflict.png",
      screenshotOptions
    );
  } finally {
    await page.unroute(savePath);
  }
});
