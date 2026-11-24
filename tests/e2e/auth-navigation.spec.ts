import { expect, test } from "@playwright/test";

import { signInOwner } from "./frontend-auth-helpers";

test.use({ trace: "off", screenshot: "off", video: "off" });

test("public landing and dedicated login are separate surfaces", async ({ page }) => {
  await page.goto("/");
  await expect(
    page.getByRole("heading", {
      name: "Nền tảng GIS đám mây thế hệ mới cho dữ liệu không gian Việt Nam."
    })
  ).toBeVisible();
  await expect(page).toHaveURL("/");

  await page.getByRole("navigation", { name: "Điều hướng trang giới thiệu" })
    .getByRole("link", { name: /^Đăng nhập/ }).click();
  await expect(
    page.getByRole("heading", { name: "Đăng nhập OpenVietGIS" })
  ).toBeVisible();
  await expect(page).toHaveURL("/login");
});

test("protected deep link returns to the intended canonical destination", async ({
  page
}) => {
  await signInOwner(page, "/app/account");
  await expect(page).toHaveURL("/app/account");
  await expect(
    page.getByRole("heading", { name: "Tài khoản & phiên đăng nhập" })
  ).toBeVisible();
});

test("external return_to is rejected and falls back to /app", async ({ page }) => {
  await page.goto(
    "/login?return_to=" +
      encodeURIComponent("https://evil.example.test/steal-session")
  );

  await page
    .getByRole("link", {
      name: "Đăng nhập với OpenVietGIS Development",
      exact: true
    })
    .click();
  await page.locator("#username").fill("owner-a");
  await page.locator("#password").fill("openvietgis-test-owner-a");
  await page.locator("#kc-login").click();

  await expect(page).toHaveURL("/app");
  await expect(page.locator(".ovg-app-shell")).toBeVisible();
});

test("sign out from a deep route invalidates the application session", async ({
  page
}) => {
  await signInOwner(page, "/app/account");

  await page.getByRole("button", { name: "Mở menu tài khoản" }).click();
  await page
    .locator(".ovg-account-dropdown")
    .getByRole("button", { name: "Đăng xuất", exact: true })
    .click();

  await expect(page).toHaveURL("/");
  await expect(
    page.getByRole("heading", {
      name: "Nền tảng GIS đám mây thế hệ mới cho dữ liệu không gian Việt Nam."
    })
  ).toBeVisible();

  await page.goto("/app/account");
  await expect.poll(() => page.url()).toContain("/login?return_to=");
});
