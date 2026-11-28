import { expect, test } from "@playwright/test";

import { signInOwner } from "./frontend-auth-helpers";

test.use({ trace: "off", screenshot: "off", video: "off" });

test("public shell exposes skip navigation and visible keyboard focus", async ({
  page
}) => {
  await page.goto("/");
  const skip = page.getByRole("link", { name: "Bỏ qua điều hướng" });
  await expect(skip).toBeAttached();
  await page.keyboard.press("Tab");
  await expect(skip).toBeFocused();
  await expect(skip).toHaveCSS("outline-style", "solid");
});

test("authenticated shell stays within viewport at reference widths", async ({
  page
}) => {
  await signInOwner(page);

  for (const width of [1440, 1024, 768]) {
    await page.setViewportSize({ width, height: 900 });
    await expect(page.locator(".live-studio")).toBeVisible();

    const overflow = await page.evaluate(
      () => document.documentElement.scrollWidth - window.innerWidth
    );
    expect(overflow).toBeLessThanOrEqual(1);
  }
});

test("primary account and command controls meet minimum touch target", async ({
  page
}) => {
  await signInOwner(page);
  await page.setViewportSize({ width: 1024, height: 900 });

  for (const locator of [
    page.getByRole("button", { name: "Menu tài khoản" }),
    page.getByRole("textbox", { name: "Dòng lệnh GIS", exact: true })
  ]) {
    const box = await locator.boundingBox();
    expect(box?.height ?? 0).toBeGreaterThanOrEqual(44);
    expect(box?.width ?? 0).toBeGreaterThanOrEqual(44);
  }
});
