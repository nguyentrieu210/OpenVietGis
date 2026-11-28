import { expect, test, type Page } from "@playwright/test";
import { signInOwner as signInOwnerWithCurrentFlow } from "./frontend-auth-helpers";

// Authentication and invitations must not be retained in browser artifacts.
test.use({ trace: "off", screenshot: "off", video: "off" });

async function signInOwner(page: Page) {
  await signInOwnerWithCurrentFlow(page);
  await expect(
    page.locator(".ovg-account-name").filter({ hasText: "Owner Alpha" })
  ).toBeVisible();
}

async function openAccountViaUi(page: Page) {
  await page.getByRole("button", { name: "Menu tài khoản" }).click();
  await page
    .getByRole("navigation", { name: "Tài khoản" })
    .getByRole("link", { name: "Tài khoản & phiên đăng nhập", exact: true })
    .click();
}

async function openInvitationsViaUi(page: Page) {
  await page.getByRole("button", { name: "Menu tài khoản" }).click();
  await page
    .getByRole("navigation", { name: "Tài khoản" })
    .getByRole("link", { name: "Lời mời", exact: true })
    .click();
}

async function holdDeniedRead(
  page: Page,
  path: string,
  status: number,
  navigate: () => Promise<void>,
  checkHidden: () => Promise<void>,
  checkDenied: () => Promise<void>,
  checkAfterDenied?: () => Promise<void>
) {
  const matches = (url: URL) => url.pathname === path;
  let release = () => {};
  const held = new Promise<void>((resolve) => {
    release = resolve;
  });
  await page.route(matches, async (route) => {
    await held;
    await route.fulfill({ status, json: { detail: "Denied" } });
  });
  try {
    const requested = page.waitForRequest(
      (request) =>
        matches(new URL(request.url())) && request.method() === "GET"
    );
    await navigate();
    await requested;
    await checkHidden();
    release();
    await checkDenied();
    await (checkAfterDenied ?? checkHidden)();
  } finally {
    release();
    await page.unroute(matches);
  }
}

test(
  "identity links never render a private cached payload during or after a denied refetch",
  async ({ page }) => {
    await signInOwner(page);
    const path = "/api/v1/me/identity-links";
    const sentinel = "synthetic-private-subject";
    const matches = (url: URL) => url.pathname === path;

    await page.route(matches, (route) =>
      route.fulfill({
        json: [
          {
            id: "synthetic-one",
            issuer: "https://identity.example.test",
            subject: sentinel
          },
          {
            id: "synthetic-two",
            issuer: "https://identity.example.test",
            subject: "synthetic-other-subject"
          }
        ]
      })
    );

    await openAccountViaUi(page);
    const privateDetails = page
      .locator("details.account-technical")
      .filter({ hasText: sentinel });
    await privateDetails.locator("summary").click();
    await expect(page.getByText(sentinel, { exact: false })).toBeVisible();
    await expect(
      page.getByRole("button", { name: "Hủy liên kết", exact: true })
    ).toHaveCount(2);

    await page.unroute(matches);

    // Focus revalidation preserves the warm query cache. The private payload
    // must disappear immediately while authorization is being re-checked.
    await holdDeniedRead(
      page,
      path,
      403,
      () =>
        page.evaluate(() => {
          window.dispatchEvent(new Event("visibilitychange"));
        }),
      async () => {
        await expect(page.getByText(sentinel, { exact: false })).toHaveCount(0);
        await expect(
          page.getByRole("button", { name: "Hủy liên kết", exact: true })
        ).toHaveCount(0);
        await expect(
          page.getByRole("link", { name: /^Liên kết / })
        ).toHaveCount(0);
      },
      async () => {
        await expect(page.getByRole("alert")).toContainText(
          "Không thể tải danh tính đã liên kết"
        );
      }
    );
  }
);

test(
  "unlinking the active identity revalidates the account even when the DELETE succeeds",
  async ({ page }) => {
    await signInOwner(page);
    const linksPath = "/api/v1/me/identity-links";
    const currentSubject = "synthetic-active-identity";
    const matches = (url: URL) => url.pathname === linksPath;
    let unlinked = false;

    await page.route(matches, async (route) => {
      if (route.request().method() === "DELETE") {
        unlinked = true;
        await route.fulfill({ status: 204 });
        return;
      }
      const remainingLink = {
        id: "synthetic-other",
        issuer: "https://identity.example.test",
        subject: "synthetic-other-subject"
      };
      await route.fulfill({
        json: unlinked
          ? [remainingLink]
          : [
              {
                id: "synthetic-active",
                issuer: "https://identity.example.test",
                subject: currentSubject
              },
              remainingLink
            ]
      });
    });

    try {
      await openAccountViaUi(page);
      await expect(
        page.getByText("Owner Alpha", { exact: false }).first()
      ).toBeVisible();

      const activeLink = page.locator("li").filter({ hasText: currentSubject });
      await expect(activeLink).toBeVisible();

      await holdDeniedRead(
        page,
        "/api/v1/me",
        401,
        () =>
          activeLink
            .getByRole("button", { name: "Hủy liên kết", exact: true })
            .click(),
        async () => {
          expect(unlinked).toBe(true);
          await expect(page.locator(".live-studio")).toBeVisible();
          await expect(
            page.getByRole("heading", {
              name: "Tài khoản & phiên đăng nhập"
            })
          ).toBeVisible();
          await expect(activeLink).toHaveCount(0);
        },
        async () => {
          await expect(
            page.getByRole("heading", { name: "Đăng nhập OpenVietGIS" })
          ).toBeVisible();
        },
        async () => {
          await expect(
            page.getByRole("heading", {
              name: "Tài khoản & phiên đăng nhập"
            })
          ).toHaveCount(0);
          await expect(page.locator(".live-studio")).toHaveCount(0);
        }
      );
    } finally {
      await page.unroute(matches);
    }
  }
);

test(
  "membership administration is unmounted while prior permissions are revalidated",
  async ({ page }) => {
    await signInOwner(page);
    const suffix = Date.now().toString();
    const organizationName = "Membership freshness " + suffix;

    await page.goto("/app/organizations");
    await page.getByLabel("Tên tổ chức").fill(organizationName);
    await page
      .getByLabel("Mã định danh tổ chức")
      .fill("membership-freshness-" + suffix);
    await page
      .getByRole("button", { name: "Tạo tổ chức", exact: true })
      .click();

    await page
      .getByRole("link", { name: organizationName, exact: true })
      .click();
    await expect(page).toHaveURL(/\/app\/organizations\/[0-9a-f-]{36}$/);

    const organizationId = new URL(page.url()).pathname.split("/").at(-1);
    expect(organizationId).toMatch(/^[0-9a-f-]{36}$/);

    const membershipLink = page.getByRole("link", {
      name: "Thành viên & phân quyền",
      exact: true
    });
    await membershipLink.click();

    await expect(
      page.getByTestId("member-list").locator("li").first()
    ).toBeVisible();
    await expect(
      page.getByRole("heading", { name: "Phân quyền tài nguyên" })
    ).toBeVisible();
    await expect(
      page.getByRole("heading", { name: "Lời mời thành viên", exact: true })
    ).toBeVisible();

    // A normal lifecycle conflict keeps safe errors and controls mounted.
    const memberMutation = (url: URL) =>
      url.pathname.startsWith(
        "/api/v1/organizations/" + organizationId + "/members/"
      );

    await page.route(memberMutation, (route) =>
      route.fulfill({ status: 409, json: { detail: "Conflict" } })
    );

    try {
      await page
        .getByTestId("member-list")
        .locator("li")
        .first()
        .getByRole("button", { name: "Tạm khóa" })
        .click();
      await expect(page.getByRole("alert")).toContainText(
        "Không thể thay đổi tư cách thành viên"
      );
      await expect(
        page.getByRole("heading", { name: "Lời mời thành viên", exact: true })
      ).toBeVisible();
      await expect(
        page.getByTestId("member-list").locator("li").first()
      ).toBeVisible();
    } finally {
      await page.unroute(memberMutation);
    }

    const invitationMutation = (url: URL) =>
      url.pathname ===
      "/api/v1/organizations/" + organizationId + "/invitations";

    await page.route(invitationMutation, (route) =>
      route.fulfill({ status: 422, json: { detail: "Unprocessable" } })
    );

    try {
      const recipientInput = page.getByLabel("Email người nhận");
      await recipientInput.fill("synthetic-recipient@example.test");
      await page
        .getByRole("button", { name: "Gửi lời mời", exact: true })
        .click();
      await expect(
        page.getByText("Không thể gửi lời mời.", { exact: true })
      ).toBeVisible();
      await expect(recipientInput).toHaveValue(
        "synthetic-recipient@example.test"
      );
      await expect(
        page.getByRole("region", { name: "Lời mời đã tạo" })
      ).toHaveCount(0);
    } finally {
      await page.unroute(invitationMutation);
    }

    await page
      .getByRole("link", { name: "← Tổ chức", exact: true })
      .click();
    const membershipLinkAgain = page.getByRole("link", {
      name: "Thành viên & phân quyền",
      exact: true
    });
    await expect(membershipLinkAgain).toBeVisible();

    await holdDeniedRead(
      page,
      "/api/v1/organizations/" +
        organizationId +
        "/effective-permissions",
      403,
      () => membershipLinkAgain.click(),
      async () => {
        await expect(page.getByTestId("member-list")).toHaveCount(0);
        await expect(
          page.getByTestId("resource-grant-list")
        ).toHaveCount(0);
        await expect(
          page.getByRole("heading", { name: "Lời mời thành viên", exact: true })
        ).toHaveCount(0);
        await expect(
          page.getByRole("region", { name: "Lời mời đã tạo" })
        ).toHaveCount(0);
      },
      async () => {
        await expect(page.getByRole("alert")).toContainText(
          "Không thể tải quyền truy cập tổ chức"
        );
      }
    );
  }
);

test(
  "invitation acceptance keeps the shell stable during account refetch and preserves login return-to on denial",
  async ({ page }) => {
    await signInOwner(page);

    await openInvitationsViaUi(page);
    await expect(page.getByLabel("Thông tin lời mời")).toBeVisible();

    const matches = (url: URL) => url.pathname === "/api/v1/me";
    let release = () => {};
    const held = new Promise<void>((resolve) => {
      release = resolve;
    });

    await page.route(matches, async (route) => {
      await held;
      await route.fulfill({ status: 401, json: { detail: "Denied" } });
    });

    try {
      const requested = page.waitForRequest(
        (request) =>
          matches(new URL(request.url())) && request.method() === "GET"
      );

      await page.evaluate(() => {
        window.dispatchEvent(new Event("visibilitychange"));
      });

      await requested;

      // Background revalidation must not flash the public shell or unmount the
      // authenticated route while the server answer is still pending.
      await expect(page.locator(".live-studio")).toBeVisible();
      await expect(page.getByLabel("Thông tin lời mời")).toBeVisible();

      release();

      await expect(
        page.getByRole("heading", { name: "Đăng nhập OpenVietGIS" })
      ).toBeVisible();
      await expect(page.locator(".live-studio")).toHaveCount(0);
      await expect(page.getByLabel("Thông tin lời mời")).toHaveCount(0);

      const login = page.getByRole("link", {
        name: "Đăng nhập với OpenVietGIS Development",
        exact: true
      });
      const href = await login.getAttribute("href");
      expect(
        new URL(href ?? "", page.url()).searchParams.get("return_to")
      ).toBe("/app/invitations/accept");
    } finally {
      release();
      await page.unroute(matches);
    }
  }
);
