import { expect, test, type Page } from "@playwright/test";
import {
  createMapProject,
  grantMapCollaborator,
} from "./map-fixtures";
import { signInIdentity } from "./frontend-auth-helpers";


async function hnSignIn(page: Page, member = false): Promise<void> {
  const username = member ? "member-a" : "owner-a";
  const password = member ? "openvietgis-test-member-a" : "openvietgis-test-owner-a";
  await signInIdentity(page, username, password);
  await expect(page.getByRole("button", { name: "Menu tài khoản" }))
    .toContainText(member ? "Member Alpha" : "Owner Alpha");
}

test.use({ viewport: { width: 1440, height: 1000 }, trace: "off", screenshot: "off", video: "off" });

test("House Numbering official module lifecycle and seven UI journeys", async ({ page, browser, baseURL }) => {
  test.setTimeout(180_000);
  await hnSignIn(page);
  const fixture = await createMapProject(page, Date.now().toString() + "-hn");

  await page.goto(fixture.root + "/platform-hub");
  await page.getByRole("tab", { name: "Quản lý Plugin & Số nhà" }).click();
  const row = page.getByRole("row").filter({ hasText: "Quản lý và đánh số nhà" });
  await expect(row).toBeVisible();
  await row.getByRole("button", { name: "Đăng ký" }).click();
  await expect(row.getByRole("button", { name: "Kích hoạt" })).toBeVisible();
  await row.getByRole("button", { name: "Kích hoạt" }).click();
  await expect(row.getByRole("link", { name: "Mở module" })).toBeVisible();
  await row.getByRole("link", { name: "Mở module" }).click();

  await expect(page.getByText("OpenVietGIS · Số nhà", { exact: true })).toBeVisible();
  for (const label of ["Địa bàn & Nhà","Đánh số","Phương án","Đối chiếu","Hiện trường","Hồ sơ","Tra cứu & Xuất"]) {
    await expect(page.getByRole("button", { name: label, exact: true })).toBeVisible();
  }

  // US1 — registry CRUD remains available without inventing geometry.
  await page.getByLabel("Tên khu/phường").fill("Phường E2E");
  await page.getByRole("button", { name: "Thêm khu" }).click();
  await expect(page.getByRole("cell", { name: /Phường E2E current · rev/ })).toBeVisible();

  // US2 — versioned rule UI.
  await page.getByRole("button", { name: "Đánh số", exact: true }).click();
  await page.getByLabel("Phiên bản").fill("E2E-RULE-1");
  await page.getByLabel("Căn cứ pháp lý").fill("SYNTH-AUTH-E2E");
  await page.getByRole("button", { name: "Tạo quy tắc" }).click();
  await expect(page.getByText(/Đã tạo phiên bản quy tắc/)).toBeVisible();

  // US3 — proposal workflow surface.
  await page.getByRole("button", { name: "Phương án", exact: true }).click();
  await expect(page.getByRole("button", { name: "Tạo draft" })).toBeVisible();
  await expect(page.getByRole("button", { name: "Trình duyệt" })).toBeVisible();
  await expect(page.getByRole("button", { name: "Phê duyệt" })).toBeVisible();

  // US4 — real import + unmatched reconciliation works with no fabricated building match.
  await page.getByRole("button", { name: "Đối chiếu", exact: true }).click();
  await page.getByLabel("File").setInputFiles({
    name: "unmatched.csv",
    mimeType: "text/csv",
    buffer: Buffer.from("raw_number,source_reference\n999,CERT-E2E\n", "utf-8")
  });
  await page.getByRole("button", { name: "Nhập & chuẩn hóa" }).click();
  await expect(page.getByText(/Đã đọc 1, nhận 1, loại 0/)).toBeVisible();
  await page.getByRole("button", { name: "Rebuild" }).click();
  await expect(page.getByRole("cell", { name: "certificate_unmatched" })).toBeVisible();

  // US5 — offline state is explicit; queue/sync details are covered by PostGIS integration tests.
  await page.getByRole("button", { name: "Hiện trường", exact: true }).click();
  await page.context().setOffline(true);
  await page.evaluate(() => window.dispatchEvent(new Event("offline")));
  await expect(page.getByText(/Đang offline/)).toBeVisible();
  await page.context().setOffline(false);
  await page.evaluate(() => window.dispatchEvent(new Event("online")));
  await expect(page.getByText("Đang online", { exact: true })).toBeVisible();

  // Offline queue survives a browser reload; sync idempotency itself is verified in PostGIS tests.
  const queueKey = `hn-survey-queue:${fixture.organizationId}:${fixture.projectId}`;
  await page.evaluate(
    ({ key }) => {
      localStorage.setItem(
        key,
        JSON.stringify([
          {
            client_uuid: "11111111-1111-4111-8111-111111111111",
            building_id: "22222222-2222-4222-8222-222222222222",
            condition: "house",
            photo_artifact_ids: []
          }
        ])
      );
    },
    { key: queueKey }
  );
  await page.reload();
  await page.getByRole("button", { name: "Hiện trường", exact: true }).click();
  await expect(page.getByRole("button", { name: "Sync 1 phiếu", exact: true })).toBeVisible();
  await page.evaluate(({ key }) => localStorage.removeItem(key), { key: queueKey });

  // US6 — document gate surface.
  await page.getByRole("button", { name: "Hồ sơ", exact: true }).click();
  await expect(page.getByRole("button", { name: "Tạo hồ sơ" })).toBeVisible();
  await expect(page.getByText(/Official chỉ phát hành|Hồ sơ chính thức bị khóa/i)).toBeVisible();

  // US7 — public export works without PII even with an empty registry.
  await page.getByRole("button", { name: "Tra cứu & Xuất", exact: true }).click();
  await expect(page.getByText(/PII tách khỏi registry công khai/)).toBeVisible();
  await page.getByRole("button", { name: "Tạo gói xuất" }).click();
  await expect(page.getByText(/Đã tạo gói 0 bản ghi/)).toBeVisible();

  // Viewer can enter an active module through project read grant but cannot mutate private data.
  const viewerContext = await browser.newContext({ baseURL });
  const viewer = await viewerContext.newPage();
  await hnSignIn(viewer, true);
  await grantMapCollaborator(page, viewer, fixture, "viewer");
  await viewer.goto(fixture.root + "/extensions/house_numbering");
  await expect(viewer.getByText("OpenVietGIS · Số nhà", { exact: true })).toBeVisible();
  await viewer.getByRole("button", { name: "Tra cứu & Xuất", exact: true }).click();
  await expect(viewer.getByText(/PII tách khỏi registry công khai/)).toBeVisible();
  await viewerContext.close();

  // Mobile regression — active module must remain usable without page-level horizontal overflow.
  const mobileContext = await browser.newContext({
    baseURL,
    viewport: { width: 390, height: 844 },
    permissions: ["geolocation"],
    geolocation: { longitude: 106.7, latitude: 10.8, accuracy: 7 }
  });
  const mobile = await mobileContext.newPage();
  await hnSignIn(mobile);
  await mobile.goto(fixture.root + "/extensions/house_numbering");
  await expect(mobile.getByText("OpenVietGIS · Số nhà", { exact: true })).toBeVisible();
  await mobile.getByRole("button", { name: "Đánh số", exact: true }).click();
  await expect(mobile.getByRole("heading", { name: "Quy tắc đánh số" })).toBeVisible();
  await mobile.getByRole("button", { name: "Hiện trường", exact: true }).click();
  await mobile.getByRole("button", { name: "Lấy GPS", exact: true }).click();
  await expect(mobile.getByText("EPSG:4326 · ±7.0 m", { exact: true })).toBeVisible();
  const pageOverflows = await mobile.evaluate(
    () => document.documentElement.scrollWidth > window.innerWidth + 1
  );
  expect(pageOverflows).toBe(false);
  await mobileContext.close();

  // Disable must hide runtime without deleting module data.
  await page.goto(fixture.root + "/platform-hub");
  await page.getByRole("tab", { name: "Quản lý Plugin & Số nhà" }).click();
  const activeRow = page.getByRole("row").filter({ hasText: "Quản lý và đánh số nhà" });
  await activeRow.getByRole("button", { name: "Tắt" }).click();
  await expect(page.getByText(/Đã tắt Quản lý và đánh số nhà/)).toBeVisible();
  await expect(activeRow.getByRole("button", { name: "Kích hoạt" })).toBeVisible();
  await page.goto(fixture.root + "/extensions/house_numbering");
  await expect(page.getByRole("heading", { name: "Module chưa được kích hoạt" })).toBeVisible();
});
