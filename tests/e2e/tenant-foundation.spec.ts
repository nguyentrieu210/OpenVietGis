import { expect, test } from "@playwright/test";
import { createOrganization, createProject, signInOwner } from "./frontend-auth-helpers";

test("checks readiness and reloads a persisted real tenant project", async ({ page }) => {
  const readiness = await page.request.get("/api/v1/health/ready", { timeout: 5_000 });
  expect(readiness.status()).toBe(200);
  expect(await readiness.json()).toEqual({ status: "ready", database: "ready" });

  await signInOwner(page);

  const suffix = Date.now().toString();
  const orgName = "Foundation tenant " + suffix;
  const organizationId = await createOrganization(page, orgName, "foundation-" + suffix);
  expect(organizationId).toMatch(/^[0-9a-f-]{36}$/);

  const projectName = "Foundation project " + suffix;
  const projectId = await createProject(page, projectName, "foundation-project-" + suffix);
  expect(projectId).toMatch(/^[0-9a-f-]{36}$/);

  await page.reload();
  await expect(page.locator(".live-studio")).toBeVisible();
  await expect(page.locator(".ovg-ws-trigger")).toContainText(orgName);
  await expect(page.locator(".ovg-ws-trigger")).toContainText(projectName);
});
