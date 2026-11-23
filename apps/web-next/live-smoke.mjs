import {chromium} from "@playwright/test";
import assert from "node:assert/strict";
const browser=await chromium.launch({headless:true,args:["--no-sandbox","--use-angle=swiftshader","--enable-unsafe-swiftshader"]});
const url="https://vps1.gpcds.site:2087/rewrite/?live=1";
try{
 const page=await browser.newPage({viewport:{width:1440,height:900}});
 const errors=[];page.on("pageerror",e=>errors.push(String(e)));
 await page.goto(url);await page.getByRole("link",{name:/Đăng nhập với/}).waitFor();await page.screenshot({path:"/tmp/ovg-next-live-login.png"});
 await page.getByRole("link",{name:/Đăng nhập với/}).click();await page.locator("#username").fill("owner-a");await page.locator("#password").fill("openvietgis-test-owner-a");await page.locator("#kc-login").click();
 await page.getByText("Owner Alpha",{exact:true}).waitFor({timeout:30000});
 const orgName="UX020 test "+Date.now(),projName="Map UX020 test";
 await page.getByTitle("Tạo tổ chức",{exact:true}).click();await page.getByLabel("Tên",{exact:true}).fill(orgName);await page.getByRole("button",{name:"Tạo và mở"}).click();await page.getByRole("heading",{name:"Chưa chọn dự án"}).waitFor();
 await page.getByTitle("Tạo dự án",{exact:true}).click();await page.getByLabel("Tên",{exact:true}).fill(projName);await page.getByRole("button",{name:"Tạo và mở"}).click();await page.getByRole("button",{name:/Kích hoạt quản lý số nhà/}).waitFor();
 const org=await page.getByLabel("Tổ chức",{exact:true}).inputValue(),pid=await page.getByLabel("Dự án",{exact:true}).inputValue();const root="/api/v1/organizations/"+org+"/projects/"+pid;
 const mutate=async(path,body)=>page.evaluate(async({path,body})=>{const c=await(await fetch("/api/v1/auth/csrf")).json();const r=await fetch(path,{method:"POST",headers:{"content-type":"application/json","x-openvietgis-csrf":c.token},body:JSON.stringify(body)});if(!r.ok)throw Error("API "+r.status);return await r.json()},{path,body});
 await page.getByRole("button",{name:"Thêm dữ liệu",exact:false}).first().click();
 const fixture={type:"FeatureCollection",features:[{type:"Feature",id:"test_point",geometry:{type:"Point",coordinates:[106.604,10.704]},properties:{name:"Point test"}}]};
 await page.getByLabel("Tệp dữ liệu").setInputFiles({name:"test_points.geojson",mimeType:"application/geo+json",buffer:Buffer.from(JSON.stringify(fixture))});
 await page.getByRole("button",{name:"Xác nhận nhập dữ liệu"}).waitFor({timeout:30000});await page.getByRole("button",{name:"Xác nhận nhập dữ liệu"}).click();
 await page.getByRole("button",{name:"Bảng thuộc tính"}).click();const first=page.locator(".live-table-row").first();await first.waitFor({timeout:30000});await first.click();await page.getByRole("button",{name:"Di chuyển điểm"}).waitFor();assert.equal(await page.getByRole("button",{name:"Di chuyển điểm"}).isEnabled(),true);
 await page.getByRole("button",{name:"Di chuyển điểm"}).click();const lng=Number(await page.getByLabel("Kinh độ",{exact:true}).inputValue());await page.getByLabel("Kinh độ",{exact:true}).fill(String(lng+0.000001));await page.getByRole("button",{name:"Lưu tọa độ"}).click();await page.getByText(/Đã lưu tọa độ trên máy chủ/).waitFor({timeout:30000});await page.waitForTimeout(1000);
 assert.ok(Math.abs(Number(await page.getByLabel("Kinh độ",{exact:true}).inputValue())-(lng+0.000001))<0.0000001);
 if(process.env.CORE_ONLY!=="1"){await page.getByRole("button",{name:/Kích hoạt quản lý số nhà/}).click();await page.getByRole("button",{name:"Kích hoạt",exact:true}).click();await page.getByRole("button",{name:"＋ Địa bàn",exact:true}).waitFor({timeout:30000});
 await page.getByRole("button",{name:"＋ Địa bàn",exact:true}).click();await page.getByLabel("Tên địa bàn").fill("Xã UX020 test");await page.getByRole("button",{name:"Lưu vào dự án"}).click();await page.getByRole("button",{name:/Xã UX020 test/}).waitFor({timeout:30000});
 await page.getByRole("button",{name:"Phương án",exact:true}).click();await page.getByRole("button",{name:"Tạo draft"}).waitFor();await page.getByRole("button",{name:"Đóng",exact:true}).click();}await page.screenshot({path:"/tmp/ovg-next-live-map.png"});
 await page.getByRole("button",{name:"Đăng xuất",exact:true}).click();await page.getByRole("link",{name:/Đăng nhập với/}).waitFor();assert.equal(errors.length,0,errors.join("\n"));
 console.log("PASS: unauthenticated login, Keycloak, create org/project, import actual layer, server coordinate changeset verified, "+(process.env.CORE_ONLY==="1"?"":"activate HN, create area, proposal modal, ")+"logout; zero page errors");
}finally{await browser.close()}
