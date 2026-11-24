import { expect, test } from "@playwright/test";

import {
  createMapProject,
  mapSignIn,
  openFixtureMap,
  registerMapReferences,
  saveFixtureMap
} from "./map-fixtures";

test.use({ trace: "off", screenshot: "off", video: "off" });

test("adapts GIS workspace across desktop tablet and mobile reference widths", async ({
  page
}) => {
  test.setTimeout(240_000);
  await page.setViewportSize({ width: 1440, height: 900 });
  await mapSignIn(page);

  const fixture = await createMapProject(
    page,
    Date.now().toString() + "-responsive"
  );
  await registerMapReferences(page, fixture, false);
  await saveFixtureMap(page, fixture);
  await openFixtureMap(page, fixture);

  const matrix = [
    { width: 1440, height: 900, mode: "desktop" },
    { width: 1280, height: 900, mode: "desktop" },
    { width: 1024, height: 768, mode: "tablet" },
    { width: 768, height: 720, mode: "tablet" },
    { width: 414, height: 820, mode: "mobile" },
    { width: 375, height: 760, mode: "mobile" }
  ] as const;

  for (const entry of matrix) {
    await page.setViewportSize({
      width: entry.width,
      height: entry.height
    });

    const layout = page.getByTestId("map-workspace-layout");
    await expect(layout).toHaveAttribute("data-responsive-mode", entry.mode);
    await expect(page.getByRole("region", { name: "Khung bản đồ" })).toBeVisible();

    const overflow = await page.evaluate(() => ({
      viewport: window.innerWidth,
      document: document.documentElement.scrollWidth,
      body: document.body.scrollWidth
    }));
    expect(
      Math.max(overflow.document, overflow.body),
      "page-wide overflow at " + entry.width + "px"
    ).toBeLessThanOrEqual(overflow.viewport + 1);
  }

  await page.setViewportSize({ width: 375, height: 760 });
  const controls = page.getByLabel("Điều khiển bố cục");
  await expect(controls).toBeVisible();

  // A fresh mobile viewport must expose the map: desktop preferences must
  // not open three overlapping sheets. Mobile switches one sheet at a time.
  const layerToggle = controls.getByRole("button", {
    name: "Lớp dữ liệu",
    exact: true
  });
  const inspectorToggle = controls.getByRole("button", {
    name: "Chi tiết",
    exact: true
  });
  const tableToggle = controls.getByRole("button", {
    name: "Bảng thuộc tính",
    exact: true
  });
  const sheets = [
    { toggle: layerToggle, selector: ".map-layout-left" },
    { toggle: inspectorToggle, selector: ".map-layout-right" },
    { toggle: tableToggle, selector: ".map-layout-table" }
  ];
  for (const sheet of sheets) {
    await expect(sheet.toggle).toHaveAttribute("aria-pressed", "false");
    await expect(page.locator(sheet.selector)).toBeHidden();
  }

  for (const opened of sheets) {
    await opened.toggle.click();
    for (const sheet of sheets) {
      const selected = sheet === opened;
      await expect(sheet.toggle).toHaveAttribute(
        "aria-pressed",
        selected ? "true" : "false"
      );
      if (selected) {
        await expect(page.locator(sheet.selector)).toBeVisible();
      } else {
        await expect(page.locator(sheet.selector)).toBeHidden();
      }
    }
  }

  await tableToggle.click();
  for (const sheet of sheets) {
    await expect(sheet.toggle).toHaveAttribute("aria-pressed", "false");
    await expect(page.locator(sheet.selector)).toBeHidden();
  }
  await expect(page.getByRole("region", { name: "Khung bản đồ" })).toBeVisible();

  // Mobile actions do not overwrite persisted desktop layout preferences.
  await page.setViewportSize({ width: 1440, height: 900 });
  await expect(controls).toBeHidden();
  for (const sheet of sheets) {
    await expect(page.locator(sheet.selector)).toBeVisible();
  }
});
