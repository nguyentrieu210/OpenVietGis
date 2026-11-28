import { expect, test } from "@playwright/test";

import { signInOwner } from "./frontend-auth-helpers";

test.use({ trace: "off", screenshot: "off", video: "off" });

test("public landing and login modal are separate surfaces", async ({ page }) => {
  await page.goto("/");
  await expect(page.getByRole("heading", { name: /OpenVietGIS.*GIS cho mọi người/i })).toBeVisible();
  await expect(page).toHaveURL("/");

  const popupPromise = page.waitForEvent("popup");
  await page.getByRole("button", { name: "Đăng nhập", exact: true }).click();
  const auth = await popupPromise;
  await expect(page.getByRole("dialog", { name: "Tài khoản OpenVietGIS" })).toBeVisible();
  await expect(auth.locator("#username")).toBeVisible();
  await auth.close();
  await expect(page).toHaveURL("/");
});

test("protected deep link returns to the intended canonical destination", async ({ page }) => {
  await signInOwner(page, "/app/account");
  await expect(page).toHaveURL("/app/account");
  await expect(page.getByRole("button", { name: "Menu tài khoản" })).toHaveAttribute("aria-expanded", "true");
  await expect(page.locator(".ovg-account-popover")).toBeVisible();
});

test("external return_to is ignored and authentication remains same-origin", async ({ page }, testInfo) => {
  await page.goto("/?return_to=" + encodeURIComponent("https://evil.example.test/steal-session"));
  await expect(page.getByRole("heading", { name: /OpenVietGIS.*GIS cho mọi người/i })).toBeVisible();
  const popupPromise = page.waitForEvent("popup");
  await page.getByRole("button", { name: "Đăng nhập", exact: true }).click();

  const auth = await popupPromise;
  await auth.locator("#username").fill("owner-a");
  await auth.locator("#password").fill("openvietgis-test-owner-a");
  await auth.locator("#kc-login").click();

  await expect(page.locator(".live-studio")).toBeVisible({timeout:20_000});
  await expect(page).toHaveURL("/app");
  expect(new URL(page.url()).origin).toBe(new URL(testInfo.project.use.baseURL as string).origin);
});

test("sign out from a deep route invalidates the application session", async ({ page }) => {
  await signInOwner(page, "/app/account");

  const account = page.getByRole("button", { name: "Menu tài khoản" });
  if ((await account.getAttribute("aria-expanded")) !== "true") await account.click();
  await page.locator(".ovg-account-popover").getByRole("button", { name: "Đăng xuất", exact: true }).click();

  await expect(page).toHaveURL("/");
  await expect(page.getByRole("heading", { name: /OpenVietGIS.*GIS cho mọi người/i })).toBeVisible();

  await page.goto("/app/account");
  await expect(page).toHaveURL("/app/account");
  await expect(page.locator(".landing-page")).toBeVisible();
  await expect(page.getByRole("button", { name: "Đăng nhập", exact: true })).toBeVisible();
});
