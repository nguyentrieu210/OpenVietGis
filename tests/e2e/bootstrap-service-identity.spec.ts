import { expect, test } from "@playwright/test";

test(
  "authenticates through OIDC and initializes the first organization exactly once",
  async ({ page }, testInfo) => {
    test.setTimeout(300_000);

    const authority = process.env.OPENVIETGIS_BOOTSTRAP_SECRET;
    if (!authority) throw new Error("Dedicated test bootstrap authority missing");
    const started = performance.now();

    const status = await page.request.get("/api/v1/bootstrap/status");
    expect(await status.json()).toEqual({ initialized: false });

    await page.goto("/app");
    const popupPromise = page.waitForEvent("popup");
    await page.getByRole("button", { name: "Đăng nhập", exact: true }).click();
    const auth = await popupPromise;
    await auth.locator("#username").fill("owner-a");
    await auth.locator("#password").fill("openvietgis-test-owner-a");
    await auth.locator("#kc-login").click();

    // Fresh installations must land directly on the bootstrap gate at /app.
    await expect(page).toHaveURL(new RegExp("/app$"));
    await expect(
      page.getByRole("heading", {
        name: "Khởi tạo hệ thống OpenVietGIS"
      })
    ).toBeVisible();

    const suffix = Date.now().toString();
    const orgName = "First Organization " + suffix;

    await page.getByLabel("Tên tổ chức").fill(orgName);
    await page
      .getByLabel("Mã định danh (slug)")
      .fill("first-" + suffix);
    await page
      .getByLabel("Mã bí mật khởi tạo (Bootstrap Secret)")
      .fill("invalid-fixture-authority");
    await page
      .getByRole("button", {
        name: "Khởi tạo hệ thống",
        exact: true
      })
      .click();

    await expect(page.getByRole("alert")).toContainText(
      "Khởi tạo thất bại"
    );
    await expect(
      page.getByLabel("Mã bí mật khởi tạo (Bootstrap Secret)")
    ).toHaveValue("");

    await page
      .getByLabel("Mã bí mật khởi tạo (Bootstrap Secret)")
      .fill(authority);

    const initialized = page.waitForResponse(
      (response) =>
        new URL(response.url()).pathname === "/api/v1/bootstrap/initialize" &&
        response.request().method() === "POST"
    );
    await page
      .getByRole("button", {
        name: "Khởi tạo hệ thống",
        exact: true
      })
      .click();

    const initializedResponse = await initialized;
    expect(initializedResponse.status()).toBe(201);
    const initializedResult = (await initializedResponse.json()) as {
      organization_id: string;
      owner_membership_id: string;
    };
    expect(initializedResult.organization_id).toMatch(/^[0-9a-f-]{36}$/);

    await expect(
      page.getByRole("heading", { name: "Chào Owner Alpha" })
    ).toBeVisible();

    // Navigate by the server-confirmed ID so this remains deterministic even
    // when an E2E database contains more organizations than one dashboard page.
    await page.goto(
      "/app/organizations/" + initializedResult.organization_id
    );
    await expect(
      page.getByRole("heading", { name: orgName, exact: true })
    ).toBeVisible();
    await expect(page.getByTestId("active-organization")).toContainText(
      "Chủ sở hữu"
    );

    const elapsedMs = performance.now() - started;
    await testInfo.attach("sc-011-bootstrap-timing", {
      body: JSON.stringify({ elapsedMs: Math.round(elapsedMs) }),
      contentType: "application/json"
    });
    expect(elapsedMs).toBeLessThan(300_000);

    const csrf = await page.request.get("/api/v1/auth/csrf");
    const denied = await page.request.post(
      "/api/v1/bootstrap/initialize",
      {
        headers: {
          Authorization: "Bearer " + authority,
          "X-OpenVietGIS-CSRF": (await csrf.json()).token
        },
        data: {
          organization_name: "Reuse",
          organization_slug: "reuse-" + suffix
        }
      }
    );

    expect(denied.status()).toBe(409);
    expect(
      await (
        await page.request.get("/api/v1/bootstrap/status")
      ).json()
    ).toEqual({ initialized: true });

    expect(await page.evaluate(() => Object.keys(localStorage))).toEqual(
      []
    );
    expect(await page.evaluate(() => Object.keys(sessionStorage))).toEqual(
      []
    );
  }
);
