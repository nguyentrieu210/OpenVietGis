import { expect, type Page } from "@playwright/test";

export async function signInOwner(
  page: Page,
  startPath = "/app"
): Promise<void> {
  await page.goto(startPath);
  await expect.poll(() => page.url()).toContain("/login?return_to=");

  await page
    .getByRole("link", {
      name: "Đăng nhập với OpenVietGIS Development",
      exact: true
    })
    .click();

  await page.locator("#username").fill("owner-a");
  await page.locator("#password").fill("openvietgis-test-owner-a");
  await page.locator("#kc-login").click();

  await expect(page.locator(".ovg-app-shell")).toBeVisible();
}

export async function createOrganization(
  page: Page,
  name: string,
  slug: string
): Promise<string> {
  await page.goto("/app/organizations");
  await page.getByLabel("Tên tổ chức").fill(name);
  await page.getByLabel("Mã định danh tổ chức").fill(slug);
  await page.getByRole("button", { name: "Tạo tổ chức" }).click();
  await page.getByRole("link", { name, exact: true }).click();
  await expect(page).toHaveURL(/\/app\/organizations\/[0-9a-f-]{36}$/);

  const segments = new URL(page.url()).pathname.split("/").filter(Boolean);
  const organizationId = segments.at(-1) ?? "";
  expect(organizationId).toBeTruthy();
  return decodeURIComponent(organizationId);
}
