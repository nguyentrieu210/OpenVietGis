import { expect, test, type Page } from "@playwright/test";
import { signInOwner } from "./frontend-auth-helpers";

async function signIn(page: Page) {
  await signInOwner(page);
  await expect(
    page.locator(".ovg-account-name").filter({ hasText: "Owner Alpha" })
  ).toBeVisible();
}

async function createOrganization(
  page: Page,
  name: string,
  slug: string
): Promise<string> {
  await page.goto("/app/organizations");
  await page.getByLabel("Tên tổ chức").fill(name);
  await page.getByLabel("Mã định danh tổ chức").fill(slug);
  await page
    .getByRole("button", { name: "Tạo tổ chức", exact: true })
    .click();

  const link = page.getByRole("link", { name, exact: true });
  await expect(link).toBeVisible();
  const href = await link.getAttribute("href");
  const id = href?.split("/").at(-1) ?? "";
  expect(id).toMatch(/^[0-9a-f-]{36}$/);
  return id;
}

async function createProject(
  page: Page,
  organizationId: string,
  name: string,
  slug: string
): Promise<string> {
  await page.goto("/app/organizations/" + organizationId + "/projects");
  await page.getByLabel("Tên dự án").fill(name);
  await page.getByLabel("Mã định danh dự án").fill(slug);
  await page
    .getByRole("button", { name: "Tạo dự án", exact: true })
    .click();

  const row = page
    .locator('li[data-testid^="project-"]')
    .filter({ hasText: name });
  await expect(row).toBeVisible();
  const id = (await row.getAttribute("data-testid"))?.slice("project-".length) ?? "";
  expect(id).toMatch(/^[0-9a-f-]{36}$/);
  return id;
}

test(
  "logs in, creates organizations, and switches explicit organization context",
  async ({ page }, testInfo) => {
    test.setTimeout(120_000);
    await signIn(page);

    const suffix = Date.now().toString();
    const names = ["Alpha Team", "Beta Team", "Gamma Team"].map(
      (name, index) => name + " " + suffix + "-" + index
    );

    const organizationIds = new Map<string, string>();
    for (const [index, name] of names.entries()) {
      const id = await createOrganization(
        page,
        name,
        "e2e-" + suffix + "-" + index
      );
      organizationIds.set(name, id);
    }
    expect(new Set(organizationIds.values()).size).toBe(3);

    // Perform three real organization switches without signing out.
    await page.goto("/app/organizations");
    const timings: Array<{ switchNumber: number; elapsedMs: number }> = [];

    for (const [index, name] of [names[2], names[0], names[1]].entries()) {
      if (!name) throw new Error("Missing switch target");
      const id = organizationIds.get(name);
      if (!id) throw new Error("Missing organization ID");

      const started = performance.now();
      await page.getByRole("link", { name, exact: true }).click();
      await expect(page).toHaveURL(
        new RegExp("/app/organizations/" + id + "$")
      );
      await expect(
        page.getByRole("heading", { name, exact: true })
      ).toBeVisible();

      // UUIDs are intentionally no longer primary labels; URL + human role proves
      // the selected context while scoped API access proves authorization.
      await expect(page.getByTestId("active-organization")).toContainText(
        "Chủ sở hữu"
      );

      const projectList = page.waitForResponse(
        (response) =>
          new URL(response.url()).pathname ===
            "/api/v1/organizations/" + id + "/projects" &&
          response.request().method() === "GET"
      );

      await page.goto("/app/organizations/" + id + "/projects");
      expect((await projectList).status()).toBe(200);
      await expect(page.getByTestId("project-list")).toBeAttached();

      const elapsedMs = performance.now() - started;
      timings.push({
        switchNumber: index + 1,
        elapsedMs: Math.round(elapsedMs)
      });
      await testInfo.attach("sc-001-switch-" + (index + 1), {
        body: JSON.stringify(timings.at(-1)),
        contentType: "application/json"
      });
      expect(elapsedMs).toBeLessThan(10_000);

      await page.goto("/app/organizations/" + id);
      await expect(
        page.getByRole("heading", { name, exact: true })
      ).toBeVisible();
      await page.goto("/app/organizations");
    }

    const organizationId = organizationIds.get(names[1] ?? "");
    if (!organizationId) {
      throw new Error("Missing organization for workspace journey");
    }

    const workspaceA1 = "Workspace A1 " + suffix;
    const workspaceA2 = "Workspace A2 " + suffix;

    await page.goto(
      "/app/organizations/" + organizationId + "/workspaces"
    );
    for (const [index, workspaceName] of [
      workspaceA1,
      workspaceA2
    ].entries()) {
      await page.getByLabel("Tên không gian làm việc").fill(workspaceName);
      await page
        .getByLabel("Mã định danh")
        .fill("workspace-" + suffix + "-" + index);
      await page
        .getByRole("button", {
          name: "Tạo không gian làm việc",
          exact: true
        })
        .click();
      await expect(
        page.getByText(workspaceName, { exact: true })
      ).toBeVisible();
    }

    await page.goto(
      "/app/organizations/" + organizationId + "/projects"
    );
    const projectName = "Project P1 " + suffix;
    await page.getByLabel("Tên dự án").fill(projectName);
    await page
      .getByLabel("Mã định danh dự án")
      .fill("project-" + suffix);
    await page
      .getByLabel("Không gian làm việc của dự án")
      .selectOption({ label: workspaceA1 });
    await page
      .getByRole("button", { name: "Tạo dự án", exact: true })
      .click();

    const projectRow = page
      .locator('li[data-testid^="project-"]')
      .filter({ hasText: projectName });
    await expect(projectRow).toBeVisible();

    const projectWorkspace = page.getByLabel(
      "Không gian làm việc của " + projectName
    );
    const a2Value = await projectWorkspace
      .getByRole("option", { name: workspaceA2 })
      .getAttribute("value");
    if (!a2Value) throw new Error("Missing destination workspace ID");

    const projectId =
      (await projectRow.getAttribute("data-testid"))?.slice(
        "project-".length
      ) ?? "";
    expect(projectId).toMatch(/^[0-9a-f-]{36}$/);

    async function assertOwnership(
      workspaceId: string | null,
      status: "active" | "archived"
    ) {
      const response = await page.request.get(
        "/api/v1/organizations/" +
          organizationId +
          "/projects/" +
          projectId
      );
      expect(response.status()).toBe(200);
      expect(await response.json()).toMatchObject({
        id: projectId,
        organization_id: organizationId,
        workspace_id: workspaceId,
        status
      });
    }

    const originalWorkspaceId = await projectWorkspace.inputValue();
    expect(originalWorkspaceId).toMatch(/^[0-9a-f-]{36}$/);
    expect(originalWorkspaceId).not.toBe(a2Value);
    await assertOwnership(originalWorkspaceId, "active");

    const projectMutationPath =
      "/api/v1/organizations/" +
      organizationId +
      "/projects/" +
      projectId;

    await projectWorkspace.selectOption({ label: workspaceA2 });
    const moved = page.waitForResponse(
      (response) =>
        new URL(response.url()).pathname === projectMutationPath &&
        response.request().method() === "PATCH"
    );
    await page
      .getByRole("button", {
        name: "Chuyển nhóm " + projectName,
        exact: true
      })
      .click();
    expect((await moved).status()).toBe(200);
    await expect(projectRow).toContainText(workspaceA2);
    await assertOwnership(a2Value, "active");

    const ungrouped = page.waitForResponse(
      (response) =>
        new URL(response.url()).pathname === projectMutationPath &&
        response.request().method() === "PATCH"
    );
    await page
      .getByRole("button", {
        name: "Bỏ khỏi nhóm " + projectName,
        exact: true
      })
      .click();
    expect((await ungrouped).status()).toBe(200);
    await expect(projectRow).toContainText("Chưa phân nhóm");
    await assertOwnership(null, "active");

    const archived = page.waitForResponse(
      (response) =>
        new URL(response.url()).pathname === projectMutationPath &&
        response.request().method() === "PATCH"
    );
    await page
      .getByRole("button", {
        name: "Lưu trữ " + projectName,
        exact: true
      })
      .click();
    expect((await archived).status()).toBe(200);
    await expect(projectRow).toContainText("Đã lưu trữ");
    await assertOwnership(null, "archived");

    const storage = await page.evaluate(() => ({
      local: Object.keys(localStorage),
      session: Object.keys(sessionStorage)
    }));
    expect(storage.local).toEqual([]);
    expect(storage.session).toEqual([]);
  }
);

test(
  "direct tenant URL substitution cannot resolve another organization's project",
  async ({ page }) => {
    test.setTimeout(90_000);
    await signIn(page);

    const suffix = Date.now().toString();
    const orgNames = [
      "Isolation A " + suffix,
      "Isolation B " + suffix
    ];
    const organizations: string[] = [];

    for (const [index, name] of orgNames.entries()) {
      organizations.push(
        await createOrganization(
          page,
          name,
          "isolation-" + suffix + "-" + index
        )
      );
    }

    const [orgA, orgB] = organizations;
    expect(orgA).toMatch(/^[0-9a-f-]{36}$/);
    expect(orgB).toMatch(/^[0-9a-f-]{36}$/);
    expect(orgA).not.toBe(orgB);

    const projectAName = "Private isolation project A " + suffix;
    const projectBName = "Private isolation project B " + suffix;

    const projectAId = await createProject(
      page,
      orgA!,
      projectAName,
      "isolation-project-a-" + suffix
    );
    const projectBId = await createProject(
      page,
      orgB!,
      projectBName,
      "isolation-project-b-" + suffix
    );

    await page.goto("/app/organizations/" + orgB + "/projects");
    await expect(
      page.getByText(projectBName, { exact: true })
    ).toBeVisible();
    await expect(
      page.getByText(projectAName, { exact: true })
    ).toHaveCount(0);

    // API tenant substitution must fail closed, despite the actor owning both orgs.
    const detailA = await page.request.get(
      "/api/v1/organizations/" + orgA + "/projects/" + projectAId
    );
    expect(detailA.status()).toBe(200);
    expect(await detailA.json()).toMatchObject({
      id: projectAId,
      organization_id: orgA
    });

    const mismatched = await page.request.get(
      "/api/v1/organizations/" + orgB + "/projects/" + projectAId
    );
    expect(mismatched.status()).toBe(404);
    const mismatchBody = await mismatched.text();
    expect(mismatchBody).not.toContain(projectAName);
    expect(mismatchBody).not.toContain(
      "isolation-project-a-" + suffix
    );

    const reverseMismatch = await page.request.get(
      "/api/v1/organizations/" + orgA + "/projects/" + projectBId
    );
    expect(reverseMismatch.status()).toBe(404);
    expect(await reverseMismatch.text()).not.toContain(projectBName);

    const detailB = await page.request.get(
      "/api/v1/organizations/" + orgB + "/projects/" + projectBId
    );
    expect(detailB.status()).toBe(200);
    expect(await detailB.json()).toMatchObject({
      id: projectBId,
      organization_id: orgB
    });
  }
);
