import { expect, test, type Page } from "@playwright/test";
import { signInIdentity } from "./frontend-auth-helpers";

test.use({ trace: "off", screenshot: "off", video: "off" });

type UserName = "owner-a" | "member-a" | "member-b";

const users = {
  "owner-a": { password: "openvietgis-test-owner-a", display: "Owner Alpha" },
  "member-a": { password: "openvietgis-test-member-a", display: "Member Alpha" },
  "member-b": { password: "openvietgis-test-member-b", display: "Member Beta" }
} as const;

async function signIn(page: Page, username: UserName) {
  await signInIdentity(page, username, users[username].password);
  await expect(
    page.locator(".ovg-account-name").filter({ hasText: users[username].display })
  ).toBeVisible();
}

async function currentUserId(page: Page): Promise<string> {
  const response = await page.request.get("/api/v1/me");
  expect(response.status()).toBe(200);
  return (await response.json() as { id: string }).id;
}

async function mutation<T>(
  page: Page,
  path: string,
  method: "POST" | "PATCH",
  body: unknown
): Promise<{ status: number; body: T | null }> {
  return page.evaluate(async ({ path, method, body }) => {
    const csrfResponse = await fetch("/api/v1/auth/csrf");
    if (!csrfResponse.ok) throw new Error("Unable to obtain mutation context");
    const csrf = await csrfResponse.json() as { token: string };
    const response = await fetch(path, {
      method,
      headers: {
        "content-type": "application/json",
        "x-openvietgis-csrf": csrf.token
      },
      body: JSON.stringify(body)
    });
    const text = await response.text();
    return {
      status: response.status,
      body: text ? JSON.parse(text) as T : null
    };
  }, { path, method, body });
}

async function createOrganization(page: Page, suffix: string): Promise<string> {
  const result = await mutation<{ organization: { id: string } }>(
    page,
    "/api/v1/organizations",
    "POST",
    { name: "Invitation team " + suffix, slug: "invitation-" + suffix }
  );
  expect(result.status).toBe(201);
  const id = result.body?.organization.id ?? "";
  expect(id).toMatch(/^[0-9a-f-]{36}$/);
  return id;
}

async function createProject(
  page: Page,
  organizationId: string,
  name: string,
  slug: string
): Promise<string> {
  const result = await mutation<{ id: string }>(
    page,
    "/api/v1/organizations/" + organizationId + "/projects",
    "POST",
    { name, slug }
  );
  expect(result.status).toBe(201);
  const id = result.body?.id ?? "";
  expect(id).toMatch(/^[0-9a-f-]{36}$/);
  return id;
}

async function pasteInvitation(page: Page, handoff: string) {
  const field = page.getByLabel("Thông tin lời mời");
  await expect(field).toBeVisible();
  await field.evaluate((element, value) => {
    if (!(element instanceof HTMLTextAreaElement)) {
      throw new Error("Expected invitation input");
    }
    const setter = Object.getOwnPropertyDescriptor(
      HTMLTextAreaElement.prototype,
      "value"
    )?.set;
    if (!setter) throw new Error("Missing textarea value setter");
    setter.call(element, value);
    element.dispatchEvent(new Event("input", { bubbles: true }));
  }, handoff);
}

async function submitInvitation(
  page: Page,
  handoff: string,
  status: number
) {
  await page.goto("/app/invitations/accept");
  await expect(
    page.getByRole("heading", { name: "Chấp nhận lời mời" })
  ).toBeVisible();
  await pasteInvitation(page, handoff);

  const response = page.waitForResponse(
    (candidate) =>
      new URL(candidate.url()).pathname === "/api/v1/invitations/accept" &&
      candidate.request().method() === "POST"
  );

  await page
    .getByRole("button", { name: "Chấp nhận lời mời", exact: true })
    .click();

  expect((await response).status()).toBe(status);
}

test(
  "recipient-bound invitation grants one project and revocation stops the next protected request",
  async ({ page: owner, browser, baseURL }, testInfo) => {
    test.setTimeout(300_000);
    const options = { baseURL: baseURL ?? "http://127.0.0.1:5178" };
    const wrongContext = await browser.newContext(options);
    const recipientContext = await browser.newContext(options);

    try {
      const wrong = await wrongContext.newPage();
      const recipient = await recipientContext.newPage();

      await signIn(owner, "owner-a");
      const suffix = Date.now().toString();
      const orgId = await createOrganization(owner, suffix);
      const projectName = "Invited project " + suffix;
      const projectId = await createProject(
        owner,
        orgId,
        projectName,
        "invited-project-" + suffix
      );
      const hiddenProjectName = "Unshared project " + suffix;
      await createProject(
        owner,
        orgId,
        hiddenProjectName,
        "unshared-project-" + suffix
      );

      const projectPath =
        "/api/v1/organizations/" + orgId + "/projects/" + projectId;

      await owner.goto("/app/organizations/" + orgId + "/membership");
      await expect(
        owner.getByRole("heading", { name: "Lời mời thành viên", exact: true })
      ).toBeVisible();

      await owner.getByLabel("Email người nhận").fill("member-a@example.test");
      await owner.getByLabel("Vai trò trong tổ chức").selectOption("member");
      await owner.getByLabel("Kèm quyền truy cập tài nguyên").check();

      const grantFields = owner.getByRole("group", {
        name: "Quyền tài nguyên trong lời mời"
      });
      await grantFields
        .getByRole("combobox", { name: "Phạm vi", exact: true })
        .selectOption("project");

      const resourceSelect = grantFields.getByRole("combobox", {
        name: "Tài nguyên",
        exact: true
      });
      await expect(
        resourceSelect.locator('option[value="' + projectId + '"]')
      ).toHaveCount(1);
      await resourceSelect.selectOption(projectId);
      await grantFields
        .getByRole("combobox", { name: "Vai trò trên tài nguyên", exact: true })
        .selectOption("viewer");

      await owner
        .getByRole("button", { name: "Gửi lời mời", exact: true })
        .click();

      const issued = owner.getByRole("region", { name: "Lời mời đã tạo" });
      await expect(issued).toBeVisible();
      const handoff = await issued.getByLabel("Thông tin lời mời").inputValue();

      let parsed: { organization_id: string; secret: string };
      try {
        parsed = JSON.parse(handoff) as typeof parsed;
      } catch {
        throw new Error("Invalid invitation handoff format");
      }
      expect(parsed.organization_id).toBe(orgId);
      expect(parsed.secret.length).toBeGreaterThanOrEqual(32);

      await issued
        .getByRole("button", { name: "Ẩn mã bí mật dùng một lần" })
        .click();
      await expect(issued).toHaveCount(0);

      // A different verified identity must not consume the recipient-bound invitation.
      await signIn(wrong, "member-b");
      const wrongUserId = await currentUserId(wrong);
      await submitInvitation(wrong, handoff, 400);
      await expect(wrong.getByRole("alert")).toContainText(
        "Không thể chấp nhận lời mời"
      );

      const wrongOrganizations = await wrong.request.get(
        "/api/v1/me/organizations"
      );
      expect(wrongOrganizations.status()).toBe(200);
      expect(
        (await wrongOrganizations.json() as Array<{ id: string }>).some(
          (org) => org.id === orgId
        )
      ).toBe(false);

      await wrong.goto("/app/organizations/" + orgId + "/projects");
      await expect(wrong.getByRole("alert")).toContainText(
        "Không thể tải quyền truy cập dự án"
      );
      await expect(wrong.getByTestId("project-list")).toHaveCount(0);
      await expect(wrong.getByText(projectName, { exact: true })).toHaveCount(0);
      await expect(
        wrong.getByText(hiddenProjectName, { exact: true })
      ).toHaveCount(0);

      const unauthorizedDetail = await wrong.goto(projectPath);
      expect(unauthorizedDetail?.status()).toBe(404);
      expect(await unauthorizedDetail?.text()).not.toContain(projectName);

      // The intended recipient accepts and sees only the granted project.
      const acceptanceStarted = performance.now();
      await signIn(recipient, "member-a");
      const recipientUserId = await currentUserId(recipient);
      await submitInvitation(recipient, handoff, 200);

      await expect(recipient).toHaveURL(
        new RegExp("/app/organizations/" + orgId + "$")
      );
      await expect(
        recipient.getByTestId("active-organization")
      ).toContainText("Thành viên");

      await recipient.goto("/app/organizations/" + orgId + "/projects");
      await expect(
        recipient.getByText(projectName, { exact: true })
      ).toBeVisible();
      await expect(
        recipient.getByText(hiddenProjectName, { exact: true })
      ).toHaveCount(0);

      const granted = await recipient.request.get(projectPath);
      expect(granted.status()).toBe(200);
      expect(await granted.json()).toMatchObject({
        id: projectId,
        organization_id: orgId
      });

      const acceptanceElapsedMs = performance.now() - acceptanceStarted;
      await testInfo.attach("sc-004-invitation-acceptance", {
        body: JSON.stringify({ elapsedMs: Math.round(acceptanceElapsedMs) }),
        contentType: "application/json"
      });
      expect(acceptanceElapsedMs).toBeLessThan(180_000);

      // One-time use must fail closed.
      await submitInvitation(recipient, handoff, 400);
      await expect(recipient.getByRole("alert")).toContainText(
        "Không thể chấp nhận lời mời"
      );

      // Populate project-list cache, then return to the organization without reload.
      await recipient.goto("/app/organizations/" + orgId + "/projects");
      await expect(
        recipient.getByText(projectName, { exact: true })
      ).toBeVisible();
      await recipient.getByRole("link", { name: "← Tổ chức", exact: true }).click();
      await expect(recipient.getByTestId("active-organization")).toBeVisible();

      // Revoke membership from the owner without replacing recipient context.
      await owner.goto("/app/organizations/" + orgId + "/membership");
      const memberList = owner.getByTestId("member-list");
      const memberRow = memberList
        .locator("li")
        .filter({ hasText: recipientUserId });

      await expect(memberRow).toContainText("Đang hoạt động");
      await expect(
        memberList.locator("li").filter({ hasText: wrongUserId })
      ).toHaveCount(0);

      owner.once("dialog", (dialog) => void dialog.accept());
      await memberRow
        .getByRole("button", { name: "Thu hồi", exact: true })
        .click();
      await expect(memberRow).toContainText("Đã thu hồi");
      expect((await recipient.request.get(projectPath)).status()).toBe(404);

      // Hold the real project-list refetch once. Previously cached project content
      // must disappear while authorization is being revalidated and stay hidden.
      const projectListPath =
        "/api/v1/organizations/" + orgId + "/projects";
      let releaseRefetch = () => {};
      const refetchGate = new Promise<void>((resolve) => {
        releaseRefetch = resolve;
      });
      const listRoute = (url: URL) => url.pathname === projectListPath;

      await recipient.route(listRoute, async (route) => {
        await refetchGate;
        await route.continue();
      });

      try {
        const requested = recipient.waitForRequest(
          (request) =>
            listRoute(new URL(request.url())) && request.method() === "GET"
        );
        const denied = recipient.waitForResponse(
          (response) =>
            listRoute(new URL(response.url())) &&
            response.request().method() === "GET"
        );

        try {
          await recipient.goto(
            "/app/organizations/" + orgId + "/projects"
          );
          await requested;
          await expect(
            recipient.getByText(projectName, { exact: true })
          ).toHaveCount(0);
        } finally {
          releaseRefetch();
        }

        expect((await denied).status()).toBe(404);
        await expect(recipient.getByRole("alert")).toContainText(
          "Không thể tải quyền truy cập dự án"
        );
        await expect(
          recipient.getByText(projectName, { exact: true })
        ).toHaveCount(0);
      } finally {
        releaseRefetch();
        await recipient.unroute(listRoute);
      }

      const storage = await recipient.evaluate(() => ({
        local: Object.keys(localStorage),
        session: Object.keys(sessionStorage)
      }));
      expect(storage.local).toEqual([]);
      expect(storage.session).toEqual([]);
    } finally {
      await wrongContext.close();
      await recipientContext.close();
    }
  }
);
