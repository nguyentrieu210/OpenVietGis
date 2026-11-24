import { expect, test } from "@playwright/test";
import { readFile } from "node:fs/promises";
import type { ExtensionRecord, ExtensionPortableBundle } from "../../packages/client-sdk/src";
import { createMapProject, mapMutation, mapRead, mapSignIn } from "./map-fixtures";

test.use({viewport:{width:1280,height:900},trace:"off",screenshot:"off",video:"off"});

test("R014 actual browser activates, edits, exports and retains extension history", async ({page}) => {
  test.setTimeout(120_000);
  await mapSignIn(page);
  const fixture = await createMapProject(page, Date.now()+"-extension");
  const registered = await mapMutation<{id:string}>(page, "/api/v1/organizations/"+fixture.organizationId+
    "/extensions/manifests", "POST", {
    extension_key:"browser_notes", display_name:"Browser notes", version:"1.0.0",
    permissions:["ext.browser_notes.read","ext.browser_notes.write"],
    entity_schemas:{note:{fields:{text:{type:"text",required:true}}}}
  });
  expect(registered.status).toBe(201);
  await page.goto(fixture.root+"/platform-hub");
  await page.getByRole("tab",{name:"Quản lý Plugin & Số nhà",exact:true}).click();
  const panel = page.getByTestId("extension-workbench");
  await expect(panel).toBeVisible();
  await panel.getByRole("button",{name:"Kích hoạt",exact:true}).click();
  await expect(panel.getByText(/Đã kích hoạt browser_notes@1.0.0/)).toBeVisible();
  const rawRecords = panel.getByText("Công cụ kỹ thuật nâng cao", { exact: false });
  await rawRecords.click();
  await panel.getByLabel("text · text (bắt buộc)",{exact:true}).fill("Initial note");
  await panel.getByRole("button",{name:"Tạo bản ghi",exact:true}).click();
  await expect(panel.getByText("Đã lưu bản ghi phiên bản 1.")).toBeVisible();
  const recordPath = fixture.api+"/extensions/browser_notes/records";
  const first = await mapRead<{items:ExtensionRecord[]}>(page,recordPath);
  expect(first.items).toHaveLength(1);
  expect(first.items[0]?.payload.text).toBe("Initial note");
  await panel.getByRole("button", { name: /note · rev 1/ }).click();
  await panel.getByLabel("text · text (bắt buộc)",{exact:true}).fill("Updated note");
  await panel.getByRole("button",{name:"Lưu thay đổi theo phiên bản",exact:true}).click();
  await expect(panel.getByText("Đã lưu bản ghi phiên bản 2.")).toBeVisible();
  const second = await mapRead<{items:ExtensionRecord[]}>(page,recordPath);
  expect(second.items[0]?.revision).toBe(2);
  expect(second.items[0]?.payload.text).toBe("Updated note");
  await expect(panel.getByText(/rev 1 · create/)).toBeVisible();
  await expect(panel.getByText(/rev 2 · update/)).toBeVisible();
  page.once("dialog",dialog=>dialog.accept());
  await panel.getByRole("button",{name:"Vô hiệu",exact:true}).click();
  await expect(panel.getByText(/Đã vô hiệu chức năng/)).toBeVisible();
  await expect(panel.getByRole("button",{name:"Tạo bản ghi",exact:true})).toHaveCount(0);
  const downloadPromise = page.waitForEvent("download");
  await panel.getByRole("button",{name:"Xuất dữ liệu & lịch sử JSON",exact:true}).click();
  const download = await downloadPromise;
  const path = await download.path();
  expect(path).toBeTruthy();
  const bundle = JSON.parse(await readFile(path!,"utf-8")) as ExtensionPortableBundle;
  expect(bundle.bundle.records).toHaveLength(1);
  expect(bundle.sha256).toMatch(/^[a-f0-9]{64}$/);
  expect(JSON.stringify(bundle.bundle)).toContain("Initial note");
  expect(JSON.stringify(bundle.bundle)).toContain("Updated note");
  expect((await mapRead<{items:ExtensionRecord[]}>(page,recordPath)).items[0]?.revision).toBe(2);
});
