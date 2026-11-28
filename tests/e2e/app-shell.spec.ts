import { expect, test } from "@playwright/test";

import {
  createOrganization,
  signInOwner
} from "./frontend-auth-helpers";

test.use({ trace: "off", screenshot: "off", video: "off" });

test("background /me revalidation keeps authenticated shell mounted", async ({
  page
}) => {
  await signInOwner(page);

  let release!: () => void;
  const held = new Promise<void>((resolve) => {
    release = resolve;
  });

  await page.route(
    (url) => url.pathname === "/api/v1/me",
    async (route) => {
      await held;
      await route.continue();
    }
  );

  const request = page.waitForRequest(
    (req) => new URL(req.url()).pathname === "/api/v1/me"
  );

  await page.evaluate(() => {
    window.dispatchEvent(new Event("visibilitychange"));
  });

  await request;
  await expect(page.locator(".live-studio")).toBeVisible();
  await expect(page.locator(".ovg-session-bootstrap")).toHaveCount(0);
  await expect(page.locator(".landing-page")).toHaveCount(0);

  release();
  await page.unroute((url) => url.pathname === "/api/v1/me");
});

test("authenticated shell persists across organization and project navigation", async ({
  page
}) => {
  await signInOwner(page);

  const suffix = Date.now().toString();
  const organizationName = "UX Shell " + suffix;
  const organizationId = await createOrganization(
    page,
    organizationName,
    "ux-shell-" + suffix
  );

  await page.locator(".live-studio").evaluate((element) => {
    element.setAttribute("data-persistence-marker", "keep");
  });

  await page.getByRole("link", { name: "Dự án", exact: true }).click();
  await expect(page).toHaveURL(
    "/app/organizations/" + organizationId + "/projects"
  );
  await expect(page.locator(".live-studio")).toHaveAttribute(
    "data-persistence-marker",
    "keep"
  );

  const projectName = "Bản đồ UX " + suffix;
  await page.getByLabel("Tên dự án").fill(projectName);
  await page.getByLabel("Mã định danh dự án").fill("ban-do-ux-" + suffix);
  await page.getByRole("button", { name: "Tạo dự án" }).click();

  await expect(page.getByText(projectName, { exact: true })).toBeVisible();
  await page
    .getByTestId("project-list")
    .locator("li")
    .filter({ hasText: projectName })
    .getByRole("link", { name: "Mở Bản đồ" })
    .click();

  await expect(page.locator(".live-studio")).toHaveAttribute(
    "data-persistence-marker",
    "keep"
  );
  await expect(page.locator(".ovg-breadcrumbs")).toContainText("Bản đồ");

  await page.goto("/app");
  await expect(page.getByText(projectName, { exact: true })).toBeVisible();
  await expect(page.getByText(organizationName, { exact: true })).toBeVisible();
  await expect(page.getByRole("link", { name: "Mở bản đồ" }).first()).toBeVisible();
});

test("legacy organization route redirects to canonical /app route", async ({
  page
}) => {
  await signInOwner(page);
  const suffix = Date.now().toString();
  const organizationId = await createOrganization(
    page,
    "Legacy UX " + suffix,
    "legacy-ux-" + suffix
  );

  await page.goto("/organizations/" + organizationId + "/projects");
  await expect(page).toHaveURL(
    "/app/organizations/" + organizationId + "/projects"
  );
});
