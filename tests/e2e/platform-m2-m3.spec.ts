import { expect, test } from "@playwright/test";
import type { SpatialFeaturePage, SpatialLayerList } from "../../packages/client-sdk/src";
import { createMapProject, mapRead, mapSignIn } from "./map-fixtures";

test.use({ viewport: { width: 1280, height: 900 }, trace: "off", screenshot: "off", video: "off" });

test("M2+M3 end-to-end: R006 Editing, R007 Query, R008 Processing, R009 Cartography, and R010–R015 Hub", async ({
  page
}) => {
  test.setTimeout(180_000);
  await mapSignIn(page);
  const fixture = await createMapProject(page, Date.now().toString() + "-m2m3");
  const base = fixture.root;

  // Seed an editable point layer via R005 UI import
  await page.goto(`${base}/transfers`);
  await expect(page.getByTestId("transfers-page")).toBeVisible();
  const geojson = JSON.stringify({
    type: "FeatureCollection",
    features: [
      {
        type: "Feature",
        id: "pt_e2e_1",
        properties: { name: "Point One" },
        geometry: { type: "Point", coordinates: [105.84, 21.02] }
      }
    ]
  });
  await page.getByTestId("import-file-input").setInputFiles({
    name: "e2e-points.geojson",
    mimeType: "application/geo+json",
    buffer: Buffer.from(geojson, "utf-8")
  });
  await expect(page.getByTestId("confirm-import-btn")).toBeEnabled({
    timeout: 15000
  });
  await page.getByTestId("confirm-import-btn").click();
  await expect(page.getByTestId("import-notice")).toContainText(
    "Nhập dữ liệu thành công",
    { timeout: 15000 }
  );

  // 1. R006 Editing Workbench
  await page.goto(`${base}/editing`);
  await expect(page.getByTestId("editing-workbench")).toBeVisible();
  await expect(page.getByTestId("native-edit-canvas")).toBeVisible();
  await page.getByLabel("Hình học 2D GeoJSON").fill('{"type":"Point","coordinates":[105.85,21.02]}');
  await page.getByLabel("Thuộc tính JSON có kiểu").fill('{"name":"Point Two"}');
  await page.getByRole("button", { name: "Đưa vào bản nháp" }).click();
  await page.getByRole("button", { name: "Lưu thay đổi", exact: true }).click();
  await expect(page.getByText(/Đã lưu đợt thay đổi/i)).toBeVisible({ timeout: 10000 });

  // 2. R007 Query Workbench
  await page.goto(`${base}/queries`);
  await expect(page.getByTestId("query-workbench")).toBeVisible();
  await page.getByLabel("Sắp xếp theo").selectOption("name");
  await page.getByLabel("Thứ tự sắp xếp").selectOption("desc");
  await page.getByRole("button", { name: "Chạy truy vấn" }).click();
  await expect(page.getByText(/Kết quả: 2 đối tượng phù hợp/i)).toBeVisible({ timeout: 10000 });
  const sortedResult = page.getByTestId("query-result");
  await expect(sortedResult.locator("tbody tr").first()).toContainText("Point Two");
  await expect(sortedResult.locator("tbody tr").last()).toContainText("Point One");

  // Compose and persist a real typed AND/NOT query, then replay it after reload.
  const first = page.getByTestId("query-filter-0");
  await first.getByRole("combobox", {name:"Trường thuộc tính",exact:true}).selectOption("name");
  await first.getByLabel("Giá trị").fill("Point One");
  await page.getByRole("button", {name:"Thêm điều kiện"}).click();
  const second = page.getByTestId("query-filter-1");
  await second.getByRole("combobox", {name:"Trường thuộc tính",exact:true}).selectOption("name");
  await second.getByLabel("Giá trị").fill("Point Two");
  await page.getByRole("combobox", {name:"Kết hợp điều kiện",exact:true}).selectOption("or");
  await page.getByRole("button", {name:"Chạy truy vấn"}).click();
  await expect(page.getByText(/Kết quả: 2 đối tượng phù hợp/i)).toBeVisible();
  await page.getByRole("combobox", {name:"Kết hợp điều kiện",exact:true}).selectOption("and");
  await second.getByLabel("Phủ định (NOT)").check();
  await page.getByLabel("Tên truy vấn lưu").fill("Browser saved query");
  await page.getByRole("button", {name:"Lưu truy vấn"}).click();
  await expect(page.getByText(/Đã lưu truy vấn Browser saved query/)).toBeVisible();
  await page.reload();
  await page.getByRole("listitem").filter({hasText:"Browser saved query"}).getByRole("button",{name:"Chạy lại truy vấn"}).click();
  await expect(page.getByText(/Kết quả: 1 đối tượng phù hợp/i)).toBeVisible();
  await expect(page.getByTestId("query-result").getByRole("cell",{name:"pt_e2e_1",exact:true})).toBeVisible();

  // 3. R008 Processing Workbench
  await page.goto(`${base}/processing`);
  await expect(page.getByTestId("processing-workbench")).toBeVisible();
  await page.getByRole("button", { name: "Chạy tác vụ" }).click();
  await expect(page.getByText(/hoàn tất \(Thành công, 2 đối tượng\)/i)).toBeVisible({
    timeout: 15000
  });

  const layers = await mapRead<SpatialLayerList>(page,fixture.api+"/layers");
  const output=layers.items.find(layer=>layer.name==="Lớp kết quả xử lý");
  expect(output).toBeDefined();
  const pins=new URLSearchParams({content_revision:output!.source.content_revision,schema_revision:output!.source.schema_revision,include_geometry:"true"});
  const derived=await mapRead<SpatialFeaturePage>(page,fixture.api+"/layers/"+output!.id+"/features?"+pins);
  expect(derived.items).toHaveLength(2);
  for(const item of derived.items) {
    expect(item.geometry?.type).toBe("Polygon");
    expect(item.attributes.buffer_meters).toBe("500");
  }

  // 4. R009 Cartography Workbench
  await page.goto(`${base}/cartography`);
  await expect(page.getByTestId("cartography-workbench")).toBeVisible();
  await page.getByRole("button", { name: "Lưu ký hiệu" }).click();
  await expect(page.getByText(/Đã lưu ký hiệu trung lập/i)).toBeVisible({ timeout: 10000 });
  await page.getByRole("button", { name: "Tạo bố cục in" }).click();
  await expect(page.getByText(/Đã tạo bố cục in A4 ngang/i)).toBeVisible({
    timeout: 10000
  });
  await page.getByRole("button", { name: "PNG 150 DPI" }).first().click();
  await expect(page.getByTestId("print-revision-preview")).toBeVisible();
  await page.getByRole("button", { name: "Xác nhận xuất PNG" }).click();
  await expect(page.getByText(/Đã xuất tệp PNG \(150 DPI/i)).toBeVisible({ timeout: 15000 });

  const pngUrl = await page.getByRole("link",{name:/Download PNG \(150 DPI/i}).first().getAttribute("href");
  expect(pngUrl).toBeTruthy();
  const png=await page.request.get(pngUrl!);
  expect(png.status()).toBe(200);
  const pngBytes = await png.body();
  expect(pngBytes.subarray(0,8).toString("hex")).toBe("89504e470d0a1a0a");
  expect(pngBytes.readUInt32BE(16)).toBe(1754);
  expect(pngBytes.readUInt32BE(20)).toBe(1240);

  // 5. R010–R015 Platform Hub (Ops Readiness + COGO)
  await page.goto(`${base}/platform-hub`);
  await page.getByRole("tab", { name: "Vận hành & Sẵn sàng" }).click();
  await expect(page.getByTestId("ops-readiness-panel")).toBeVisible();
  await expect(
    page.getByRole("cell", {
      name: "Vận hành sản xuất & tự triển khai",
      exact: true
    })
  ).toBeVisible({ timeout: 10000 });
  await expect(page.getByText(/Trạng thái phụ thuộc:\s*Sẵn sàng/i)).toBeVisible({
    timeout: 10000
  });
  const opsPanel = page.getByTestId("ops-readiness-panel");
  await expect(opsPanel.getByText(/Hồ sơ phát hành:\s*Chưa nghiệm thu/i)).toBeVisible();
  await expect(opsPanel.getByRole("cell", { name: "Chưa nghiệm thu", exact: true })).toHaveCount(15);

  // R011 genuine post-commit review; comments anchored to immutable evidence.
  await page.getByRole("tab", {name:"Cộng tác nhóm & Duyệt", exact:true}).click();
  const collaboration = page.getByTestId("collaboration-workbench");
  await expect(collaboration).toBeVisible();
  const importedLayer = layers.items.find(layer => layer.source.kind === "imported_file");
  expect(importedLayer).toBeDefined();
  await collaboration.getByRole("combobox", {name:"Nguồn dữ liệu",exact:true}).selectOption(importedLayer!.source_id);
  const changed = collaboration.getByRole("combobox", {name:"Bộ thay đổi",exact:true});
  await expect(changed.getByRole("option")).toHaveCount(2);
  await changed.selectOption({index:1});
  await collaboration.getByRole("button", {name:"Chọn tài khoản của tôi"}).click();
  await collaboration.getByRole("button", {name:"Gửi yêu cầu duyệt"}).click();
  await expect(collaboration.getByRole("region", {name:"Chi tiết yêu cầu duyệt"})).toBeVisible();
  await collaboration.getByLabel("Đối tượng đã thay đổi").selectOption("feat_new_1");
  await collaboration.getByLabel("Nội dung bình luận").fill("Đã kiểm tra điểm mới");
  await collaboration.getByRole("button", {name:"Gửi bình luận"}).click();
  await expect(collaboration.getByText(/Đã kiểm tra điểm mới/)).toBeVisible();
  await collaboration.getByLabel("Ý kiến", {exact:true}).fill("Đã đối chiếu");
  await collaboration.getByRole("button", {name:"Phê duyệt", exact:true}).click();
  await expect(collaboration.getByText("Ý kiến quyết định: Đã đối chiếu")).toBeVisible();

  await page.getByRole("tab", { name: "AutoCAD 2D (DXF) & COGO", exact: true }).click();
  await page.getByRole("button", { name: "Tính tọa độ điểm cuối" }).click();
  await expect(
    page.getByText(/Tọa độ điểm tính được: \(500100\.0000, 2300000\.0000\) · tính thử, chưa gắn CRS/i)
  ).toBeVisible();
});
