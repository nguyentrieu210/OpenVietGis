import { expect, type Page } from "@playwright/test";

export async function signInIdentity(
  page: Page,
  username: string,
  password: string,
  startPath = "/app"
): Promise<void> {
  await page.goto(startPath);
  await expect(page.locator(".landing-page")).toBeVisible();
  expect(new URL(page.url()).pathname).toBe(startPath);

  const popupPromise = page.waitForEvent("popup");
  await page.getByRole("button", { name: "Đăng nhập", exact: true }).click();
  const auth = await popupPromise;
  await auth.locator("#username").fill(username);
  await auth.locator("#password").fill(password);
  await auth.locator("#kc-login").click();

  await expect(page.locator(".live-studio")).toBeVisible({timeout:20_000});
}

export async function signInOwner(
  page: Page,
  startPath = "/app"
): Promise<void> {
  await signInIdentity(page, "owner-a", "openvietgis-test-owner-a", startPath);
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
