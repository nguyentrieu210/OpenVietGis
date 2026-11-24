import { expect, test } from "@playwright/test";
import {
  createMapProject, grantMapCollaborator, mapMutation, mapSignIn, openFixtureMap,
  registerMapReferences, saveFixtureMap
} from "./map-fixtures";

// Dedicated real authorization/read-scope regression, not a performance trial.
// Never retain secret-bearing browser traces, screenshots, or intercepted payloads.
test.use({viewport:{width:1280,height:900},trace:"off",screenshot:"off",video:"off"});

test("a delayed detail result cannot replace a new layer selection and reconnect revocation clears a warm catalog", async ({page:owner,browser,baseURL})=>{
  test.setTimeout(180_000);
  await mapSignIn(owner);
  const fixture=await createMapProject(owner,Date.now().toString()+"-protected");
  await registerMapReferences(owner,fixture);
  await saveFixtureMap(owner,fixture);
  const points=fixture.layers.find(l=>l.name==="Points")!;
  const viewerContext=await browser.newContext({baseURL,viewport:{width:1280,height:900}});
  const viewer=await viewerContext.newPage();
  let release=()=>{};
  try {
    await mapSignIn(viewer,true);
    const grant=await grantMapCollaborator(owner,viewer,fixture,"viewer");
    await openFixtureMap(viewer,fixture);
    await viewer.getByRole("button",{name:"Chọn Points làm lớp hiện hành",exact:true}).click();
    const table=viewer.getByRole("table",{name:"Bảng thuộc tính theo phiên bản"});
    await expect(table.getByRole("button",{name:"point-000001",exact:true})).toBeVisible();

    // Fetch an actual authorized response, but deliver it after the viewer has
    // switched logical layer and established a different focused feature.
    // The app must never identify this obsolete response as the current record.
    const isPointDetail=(url:URL)=>url.pathname===fixture.api+"/layers/"+points.id+"/features/point-000001";
    let held=false,delivered=false;
    const gate=new Promise<void>(resolve=>{release=resolve;});
    await viewer.route(isPointDetail,async route=>{
      const actual=await route.fetch();
      expect(actual.status()).toBe(200);
      const payload=await actual.json();
      expect(payload.context.layer_id).toBe(points.id);
      expect(payload.feature.feature_id).toBe("point-000001");
      held=true;
      await gate;
      await route.fulfill({response:actual});
      delivered=true;
    });
    await table.getByRole("button",{name:"point-000001",exact:true}).click();
    await expect.poll(()=>held).toBe(true);
    await viewer.getByRole("button",{name:"Chọn Lines làm lớp hiện hành",exact:true}).click();
    await table.getByRole("button",{name:"line-000001",exact:true}).click();
    const inspector=viewer.getByTestId("map-inspector");
    await expect(inspector).toHaveAttribute("data-feature-id","line-000001");
    release();
    await expect.poll(()=>delivered).toBe(true);
    await viewer.unroute(isPointDetail);
    await expect(inspector).toHaveAttribute("data-feature-id","line-000001");
    await expect(table.getByRole("button",{name:"point-000001",exact:true})).toHaveCount(0);

    // A warm catalog/dialog must be readable while the grant is present.
    // Reconnect revalidation must not require a manual button click.
    await viewer.getByRole("navigation",{name:"Lệnh bản đồ"}).getByRole("button",{name:"Thêm dữ liệu",exact:true}).click();
    const dialog=viewer.getByRole("dialog",{name:"Thêm dữ liệu"});
    await expect(dialog).toBeVisible();
    await expect(dialog.locator("li").filter({hasText:"Points"})).toContainText("Đã có trên bản đồ");
    await viewerContext.setOffline(true);
    const revoked=await mapMutation(owner,"/api/v1/organizations/"+fixture.organizationId+"/grants/"+grant+"/revoke","POST");
    expect(revoked.status).toBe(204);

    const denied=viewer.waitForResponse(response=>
      new URL(response.url()).pathname===fixture.api+"/map" && response.request().method()==="GET"
    );
    await viewerContext.setOffline(false);
    expect((await denied).status()).toBe(403);
    await expect(viewer.getByTestId("map-workspace")).toHaveCount(0);
    await expect(viewer.getByTestId("map-canvas")).toHaveCount(0);
    await expect(table).toHaveCount(0);
    await expect(inspector).toHaveCount(0);
    await expect(dialog).toHaveCount(0);

    // A fresh direct navigation in the same actor session cannot restore warmed
    // map state. Owner remains authorized; revocation did not alter shared map.
    await viewer.reload();
    await expect(viewer.getByRole("alert").filter({hasText:"Không có quyền truy cập bản đồ dự án này"})).toBeVisible();
    await expect(viewer.getByTestId("map-workspace")).toHaveCount(0);
    expect((await owner.request.get(fixture.api+"/map")).status()).toBe(200);
  } finally {
    release();
    await viewerContext.close();
  }
});

test("one mismatched metadata response is concealed without blocking the other authorized references", async ({page})=>{
  test.setTimeout(180_000);
  await mapSignIn(page);
  const fixture=await createMapProject(page,Date.now().toString()+"-bad-metadata");
  await registerMapReferences(page,fixture);
  await saveFixtureMap(page,fixture);
  const points=fixture.layers.find(layer=>layer.name==="Points")!;
  const lines=fixture.layers.find(layer=>layer.name==="Lines")!;
  const isPoints=(url:URL)=>url.pathname===fixture.api+"/layers/"+points.id;
  let altered=false,pointViewportRequested=false;
  page.on("request",request=>{
    if(new URL(request.url()).pathname===fixture.api+"/layers/"+points.id+"/viewport")pointViewportRequested=true;
  });
  // Fetch a real, authorized payload, then model a wrong layer in a successful
  // response; neither its name nor geometry may be treated as the requested layer.
  await page.route(isPoints,async route=>{
    const actual=await route.fetch();
    expect(actual.status()).toBe(200);
    const payload=await actual.json();
    expect(payload.id).toBe(points.id);
    altered=true;
    await route.fulfill({response:actual,json:{...payload,id:lines.id,name:"WRONG RESPONSE NAME"}});
  });
  await openFixtureMap(page,fixture);
  expect(altered).toBe(true);
  expect(pointViewportRequested).toBe(false);
  const unavailable=page.getByTestId("map-layer-"+points.id);
  await expect(unavailable).toContainText("Không khả dụng");
  await expect(unavailable).not.toContainText("WRONG RESPONSE NAME");
  await page.getByRole("textbox",{name:"Nhập lệnh bản đồ"}).fill("Mở lớp");
  await expect(page.getByRole("option").filter({ hasText: "Mở lớp: WRONG RESPONSE NAME" })).toHaveCount(0);
  await page.getByRole("option").filter({ hasText: "Mở lớp: Lines" }).click();
  await expect(page.getByTestId("accessible-map-summary")).toContainText("Lớp hiện hành: Lines");
  const table=page.getByRole("table",{name:"Bảng thuộc tính theo phiên bản"});
  await table.getByRole("button",{name:"line-000001",exact:true}).click();
  await expect(page.getByTestId("map-inspector")).toHaveAttribute("data-feature-id","line-000001");
  await page.unroute(isPoints);
  await page.getByRole("button", { name: /^Cấu hình bản đồ dùng chung/ }).click();
  await page.getByRole("button",{name:"Kiểm tra lại quyền truy cập",exact:true}).click();
  await expect(unavailable).toContainText("Points");
  await expect(page.getByTestId("map-engine-ready")).toHaveAttribute("data-ready","true");

  // A settled wrong outer map scope must not mount the workspace or leave a
  // misleading indefinite loading state.
  await page.route(url=>url.pathname===fixture.api+"/map",async route=>{
    const actual=await route.fetch();
    expect(actual.status()).toBe(200);
    const payload=await actual.json();
    expect(payload.project_id).toBe(fixture.projectId);
    await route.fulfill({response:actual,json:{...payload,project_id:"wrong-project"}});
  });
  await page.reload();
  await expect(page.getByRole("alert").filter({hasText:"Không thể xác nhận phạm vi dữ liệu và quyền truy cập bản đồ dự án này"})).toBeVisible();
  await expect(page.getByTestId("map-workspace")).toHaveCount(0);
});
