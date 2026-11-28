import { expect, type Page } from "@playwright/test";

export async function signInIdentity(
  page: Page,
  username: string,
  password: string,
  startPath = "/app"
): Promise<void> {
  await page.goto(startPath);
  await page.getByRole("link", { name: /Đăng nhập với/ }).waitFor();
  await page.getByRole("link", { name: /Đăng nhập với/ }).click();
  await page.locator("#username").fill(username);
  await page.locator("#password").fill(password);
  await page.locator("#kc-login").click();

  await expect(page.getByText("Owner Alpha", { exact: true })).toBeVisible({ timeout: 30_000 });
  await expect(page.locator(".live-studio")).toBeVisible({ timeout: 20_000 });
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
  _slug: string
): Promise<string> {
  await page.goto("/app");
  await expect(page.locator(".live-studio")).toBeVisible();

  await page.locator(".ovg-ws-trigger").click();
  const picker = page.getByRole("dialog", { name: "Tổ chức và dự án" });
  await picker.getByRole("button", { name: /Tạo tổ chức/ }).click();

  const form = page.getByRole("dialog", { name: "Tạo tổ chức" });
  const created = page.waitForResponse(response =>
    new URL(response.url()).pathname === "/api/v1/organizations" &&
    response.request().method() === "POST"
  );
  await form.getByLabel("Tên", { exact: true }).fill(name);
  await form.getByRole("button", { name: "Tạo và mở" }).click();
  const response = await created;
  expect(response.status()).toBe(201);
  const body = await response.json() as { organization: { id: string } };
  const organizationId = body.organization.id;
  expect(organizationId).toMatch(/^[0-9a-f-]{36}$/);
  await expect(form).toHaveCount(0);
  return organizationId;
}

export async function createProject(
  page: Page,
  name: string,
  _slug?: string
): Promise<string> {
  await page.locator(".ovg-ws-trigger").click();
  const picker = page.getByRole("dialog", { name: "Tổ chức và dự án" });
  await picker.getByRole("button", { name: /Dự án mới/ }).click();

  const form = page.getByRole("dialog", { name: "Tạo dự án" });
  const created = page.waitForResponse(response =>
    /\/api\/v1\/organizations\/[^/]+\/projects$/.test(new URL(response.url()).pathname) &&
    response.request().method() === "POST"
  );
  await form.getByLabel("Tên", { exact: true }).fill(name);
  await form.getByRole("button", { name: "Tạo và mở" }).click();
  const response = await created;
  expect(response.status()).toBe(201);
  const body = await response.json() as { id: string };
  expect(body.id).toMatch(/^[0-9a-f-]{36}$/);
  await expect(form).toHaveCount(0);
  return body.id;
}

export async function selectOrganization(page: Page, name: string): Promise<void> {
  await page.locator(".ovg-ws-trigger").click();
  const picker = page.getByRole("dialog", { name: "Tổ chức và dự án" });
  await picker.getByRole("button", { name, exact: true }).click();
  await expect(page.locator(".ovg-ws-trigger")).toContainText(name);
  await picker.getByRole("button", { name: "Đóng", exact: true }).click();
}

export async function selectProject(page: Page, name: string): Promise<void> {
  await page.locator(".ovg-ws-trigger").click();
  const picker = page.getByRole("dialog", { name: "Tổ chức và dự án" });
  await picker.getByRole("button", { name: new RegExp(name) }).click();
  await expect(page.locator(".ovg-ws-trigger")).toContainText(name);
}
