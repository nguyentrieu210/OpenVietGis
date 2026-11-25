import { expect, test } from "@playwright/test";

import { signInOwner } from "./frontend-auth-helpers";

test.use({ trace: "off", video: "off" });

test("landing visual baseline", async ({ page }) => {
  await page.setViewportSize({ width: 1440, height: 900 });
  await page.goto("/");
  await expect(page).toHaveScreenshot("landing.png", {
    fullPage: true,
    animations: "disabled"
  });
});

test("login visual baseline", async ({ page }) => {
  await page.setViewportSize({ width: 1440, height: 900 });
  await page.goto("/login");
  await expect(
    page.getByRole("link", { name: "Đăng nhập với OpenVietGIS Development", exact: true })
  ).toBeVisible({ timeout: 15_000 });
  await expect(page).toHaveScreenshot("login.png", {
    fullPage: true,
    animations: "disabled"
  });
});

test("authenticated dashboard and account menu visual baselines", async ({
  page
}) => {
  await page.setViewportSize({ width: 1440, height: 900 });

  const organizationId = "11111111-1111-4111-8111-111111111111";
  const projectId = "22222222-2222-4222-8222-222222222222";

  // Keep the visual baseline independent from records created by earlier E2E
  // cases while still exercising the real authenticated shell/session.
  await page.route("**/api/v1/me/organizations", async (route) => {
    if (route.request().method() !== "GET") {
      await route.continue();
      return;
    }
    await route.fulfill({
      json: [
        {
          id: organizationId,
          name: "Trung tâm GIS Việt Nam",
          slug: "trung-tam-gis-viet-nam",
          role: "owner",
          status: "active"
        }
      ]
    });
  });

  await page.route(
    "**/api/v1/organizations/" + organizationId + "/projects",
    async (route) => {
      if (route.request().method() !== "GET") {
        await route.continue();
        return;
      }
      await route.fulfill({
        json: [
          {
            id: projectId,
            organization_id: organizationId,
            workspace_id: null,
            name: "Dự án Bản đồ Hà Nội",
            slug: "du-an-ban-do-ha-noi",
            status: "active"
          }
        ]
      });
    }
  );

  await signInOwner(page);
  await expect(
    page.getByText("Dự án Bản đồ Hà Nội", { exact: true })
  ).toBeVisible();

  await expect(page).toHaveScreenshot("dashboard.png", {
    fullPage: true,
    animations: "disabled"
  });

  await page.getByRole("button", { name: "Mở menu tài khoản" }).click();
  await expect(page).toHaveScreenshot("account-menu.png", {
    fullPage: true,
    animations: "disabled"
  });
});
