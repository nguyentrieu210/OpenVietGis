import { expect, test } from "@playwright/test";

import {
  createMapProject,
  mapSignIn,
  openFixtureMap,
  registerMapReferences,
  saveFixtureMap
} from "./map-fixtures";

test.use({
  viewport: { width: 1440, height: 900 },
  trace: "off",
  screenshot: "off",
  video: "off"
});

test("supports keyboard and textual alternatives across the GIS workspace", async ({
  page
}) => {
  test.setTimeout(240_000);
  await page.emulateMedia({ reducedMotion: "reduce" });
  await mapSignIn(page);

  const fixture = await createMapProject(
    page,
    Date.now().toString() + "-accessibility"
  );
  await registerMapReferences(page, fixture, true);
  await saveFixtureMap(page, fixture);
  await openFixtureMap(page, fixture);

  const mapRegion = page.getByRole("region", { name: "Khung bản đồ" });
  await expect(mapRegion).toBeVisible();
  await expect(page.getByTestId("accessible-map-summary")).toContainText(
    "lớp đang hiển thị"
  );

  // MapLibre remains keyboard-enabled and exposes visible zoom/compass controls.
  const mapCanvas = page.locator(".maplibregl-canvas");
  await expect(mapCanvas).toHaveAttribute("tabindex", "0");
  await expect(page.locator(".maplibregl-ctrl-zoom-in")).toBeVisible();
  await expect(page.locator(".maplibregl-ctrl-zoom-out")).toBeVisible();

  const cameraBefore = await page.getByTestId("map-camera-summary").innerText();
  await mapCanvas.focus();
  await page.keyboard.press("ArrowRight");
  await expect(page.getByTestId("map-camera-summary")).not.toHaveText(
    cameraBefore
  );

  // Layer ordering has a non-drag keyboard path.
  const lines = fixture.layers.find((layer) => layer.name === "Lines")!;
  const orderBefore = await page
    .locator("[data-testid^=map-layer-]")
    .evaluateAll((nodes) => nodes.map((node) => node.getAttribute("data-testid")));
  const moveUp = page.getByRole("button", {
    name: "Đưa Lines lên trên",
    exact: true
  });
  await moveUp.focus();
  await page.keyboard.press("Enter");
  const orderAfter = await page
    .locator("[data-testid^=map-layer-]")
    .evaluateAll((nodes) => nodes.map((node) => node.getAttribute("data-testid")));
  expect(orderAfter).not.toEqual(orderBefore);
  expect(orderAfter).toContain("map-layer-" + lines.id);

  // Panel resize is keyboard-operable.
  const separator = page.getByRole("separator", {
    name: "Đổi độ rộng bảng lớp dữ liệu"
  });
  const widthBefore = await separator.getAttribute("aria-valuenow");
  await separator.focus();
  await page.keyboard.press("ArrowRight");
  await expect(separator).not.toHaveAttribute("aria-valuenow", widthBefore ?? "");

  // Table remains the textual feature alternative.
  await expect(
    page.getByRole("table", { name: "Bảng thuộc tính theo phiên bản" })
  ).toBeVisible();

  // Technical drawer closes back to its invoking control.
  const technicalTrigger = page.getByRole("button", {
    name: "Thông tin kỹ thuật",
    exact: true
  });
  await technicalTrigger.focus();
  await page.keyboard.press("Enter");
  const drawer = page.getByRole("dialog", { name: "Thông tin kỹ thuật" });
  await expect(drawer).toBeVisible();
  await drawer
    .getByRole("button", { name: "Đóng thông tin kỹ thuật" })
    .click();
  await expect(technicalTrigger).toBeFocused();

  // Reduced-motion media preference is honored by workspace CSS.
  const motion = await page.getByTestId("map-workspace").evaluate((element) => {
    const style = getComputedStyle(element);
    return {
      animationDuration: style.animationDuration,
      transitionDuration: style.transitionDuration
    };
  });
  const durationMs = (value: string) =>
    value.endsWith("ms")
      ? Number.parseFloat(value)
      : Number.parseFloat(value) * 1000;
  expect(durationMs(motion.animationDuration)).toBeLessThanOrEqual(0.02);
  expect(durationMs(motion.transitionDuration)).toBeLessThanOrEqual(0.02);

  // Focus indication is not color-only/hidden.
  await page
    .getByRole("complementary", { name: "Thanh công cụ GIS dọc" })
    .getByRole("button", { name: "Lớp", exact: true })
    .focus();
  const focusStyle = await page
    .getByRole("complementary", { name: "Thanh công cụ GIS dọc" })
    .getByRole("button", { name: "Lớp", exact: true })
    .evaluate((element) => {
      const style = getComputedStyle(element);
      return {
        outlineStyle: style.outlineStyle,
        outlineWidth: style.outlineWidth
      };
    });
  expect(focusStyle.outlineStyle).not.toBe("none");
  expect(focusStyle.outlineWidth).not.toBe("0px");

  // Search activates an exact currently authorized map layer and exposes the
  // observed (not inferred) content/schema pins only in the secondary inspector.
  async function searchCommand(label: string) {
    await page.getByRole("textbox", { name: "Nhập lệnh bản đồ" }).fill(label);
    await page.getByRole("option").filter({ hasText: label }).click();
  }
  await searchCommand("Mở lớp: Points");
  await expect(page.getByTestId("accessible-map-summary")).toContainText("Lớp hiện hành: Points");
  await page.getByRole("button", { name: "point-000001", exact: true }).click();
  const revision = page.getByTestId("inspector-observed-revision");
  await expect(revision).toBeVisible();
  expect(await revision.getAttribute("open")).toBeNull();
  await revision.getByText("Phiên bản dữ liệu đã đọc").click();
  const pointSource = fixture.layers.find(layer => layer.name === "Points")!.source;
  await expect(revision).toContainText(pointSource.content_revision);
  await expect(revision).toContainText(pointSource.schema_revision);

  // Search navigation must use the same router blocker as header links; the
  // non-drag reorder above deliberately left the map unsaved.
  await searchCommand("Mở Nhập / Xuất dữ liệu");
  const blocker = page.getByRole("dialog", { name: "Có thay đổi bản đồ chưa lưu" });
  await expect(blocker).toBeVisible();
  await blocker.getByRole("button", { name: "Ở lại" }).click();
  await expect(page.getByTestId("map-workspace")).toBeVisible();
  await searchCommand("Mở Nhập / Xuất dữ liệu");
  await blocker.getByRole("button", { name: "Rời đi và bỏ thay đổi" }).click();
  await expect(page).toHaveURL(new RegExp(fixture.root + "/transfers$"));

  for (const [label, path] of [
    ["Mở danh mục lớp dữ liệu", fixture.root + "/spatial"],
    ["Mở Thành viên & Phân quyền", "/app/organizations/" + fixture.organizationId + "/membership"],
    ["Mở danh sách dự án", "/app/organizations/" + fixture.organizationId + "/projects"]
  ] as const) {
    await openFixtureMap(page, fixture);
    await searchCommand(label);
    await expect(page).toHaveURL(new RegExp(path + "$"));
  }
});
