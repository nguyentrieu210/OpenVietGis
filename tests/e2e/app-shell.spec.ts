import { expect, test } from "@playwright/test";

import {
  createOrganization,
  createProject,
  selectProject,
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
    window.dispatchEvent(new Event("focus"));
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

  const projectName = "Bản đồ UX " + suffix;
  const projectId = await createProject(page, projectName, "ban-do-ux-" + suffix);
  await selectProject(page, projectName);

  await expect(page).toHaveURL(
    new RegExp("/app/organizations/" + organizationId + "/projects/" + projectId + "/spatial$")
  );
  await expect(page.locator(".live-studio")).toHaveAttribute(
    "data-persistence-marker",
    "keep"
  );
  await expect(page.locator(".ovg-ws-trigger")).toContainText(projectName);

  await page.goto("/app");
  await expect(page.locator(".live-studio")).toBeVisible();
  await expect(page.locator(".ovg-ws-trigger")).toContainText(organizationName);
  await expect(page.locator(".ovg-ws-trigger")).toContainText(projectName);
});

test("legacy organization route resolves the canonical workspace", async ({
  page
}) => {
  await signInOwner(page);
  const suffix = Date.now().toString();
  const organizationName = "Legacy UX " + suffix;
  const organizationId = await createOrganization(
    page,
    organizationName,
    "legacy-ux-" + suffix
  );

  await page.goto("/organizations/" + organizationId + "/projects");
  await expect(page.locator(".live-studio")).toBeVisible();
  await expect(page.locator(".ovg-ws-trigger")).toContainText(organizationName);
  await expect(page).toHaveURL(
    new RegExp("/organizations/" + organizationId + "/projects$")
  );
});
