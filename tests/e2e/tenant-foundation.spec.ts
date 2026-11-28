import { expect, test } from "@playwright/test";
import { signInOwner } from "./frontend-auth-helpers";

test("checks readiness and reloads a persisted real tenant project", async ({ page }) => {
  const readiness = await page.request.get("/api/v1/health/ready", { timeout: 5_000 });
  expect(readiness.status()).toBe(200);
  expect(await readiness.json()).toEqual({ status: "ready", database: "ready" });

  await signInOwner(page);

  const suffix = Date.now();
  const orgName = "Foundation tenant " + suffix;

  await page.goto("/app/organizations");
  await page.getByLabel("Tên tổ chức").fill(orgName);
  await page.getByLabel("Mã định danh tổ chức").fill("foundation-" + suffix);
  await page
    .getByRole("button", { name: "Tạo tổ chức", exact: true })
    .click();

  const orgLink = page.getByRole("link", { name: orgName, exact: true });
  await expect(orgLink).toBeVisible();
  await orgLink.click();
  await expect(page).toHaveURL(/\/app\/organizations\/[0-9a-f-]{36}$/);

  const organizationId = new URL(page.url()).pathname.split("/").at(-1) ?? "";
  expect(organizationId).toMatch(/^[0-9a-f-]{36}$/);

  await page.goto("/app/organizations/" + organizationId + "/projects");

  const projectName = "Foundation project " + suffix;
  await page.getByLabel("Tên dự án").fill(projectName);
  await page
    .getByLabel("Mã định danh dự án")
    .fill("foundation-project-" + suffix);
  await page
    .getByRole("button", { name: "Tạo dự án", exact: true })
    .click();

  const record = page
    .locator('li[data-testid^="project-"]')
    .filter({ hasText: projectName });
  await expect(record).toBeVisible();

  const id = await record.getAttribute("data-testid");
  expect(id).toMatch(/^project-[0-9a-f-]{36}$/);
  if (!id) throw new Error("Missing persisted project ID");

  await page.reload();
  await expect(page.getByTestId(id)).toContainText(projectName);
});
