import {chromium} from "@playwright/test";
import assert from "node:assert/strict";
const browser=await chromium.launch({headless:true,args:["--no-sandbox","--use-angle=swiftshader","--enable-unsafe-swiftshader"]});
try{
 const page=await browser.newPage({viewport:{width:1440,height:900},locale:"vi-VN"});const errors=[];page.on("pageerror",e=>errors.push(String(e)));
 await page.goto("https://vps1.gpcds.site:2087/rewrite/?live=1");await page.getByRole("link",{name:/Đăng nhập với/}).click();await page.locator("#username").fill("owner-a");await page.locator("#password").fill("openvietgis-test-owner-a");await page.locator("#kc-login").click();await page.getByRole("button",{name:"Đăng xuất",exact:true}).waitFor();
 await page.getByLabel("Tổ chức",{exact:true}).selectOption({label:"Map team 1795327408970-hn"});
 await page.getByLabel("Dự án",{exact:true}).selectOption({label:"Map project 1795327408970-hn"});
 await page.getByRole("button",{name:"＋ Địa bàn",exact:true}).waitFor({timeout:20000});
 await page.getByRole("button",{name:"＋ Thêm dữ liệu",exact:true}).click();await page.getByRole("button",{name:"Nạp bộ dữ liệu mẫu",exact:true}).click();
 await page.getByText("Đã nạp 5 lớp GIS và cây số nhà mẫu",{exact:true}).waitFor({timeout:120000});
 await page.getByRole("button",{name:/Xã Bình Minh/}).waitFor();await page.getByRole("button",{name:/Đường Nguyễn Huệ/}).waitFor();await page.getByRole("button",{name:/Hẻm 155\/3/}).waitFor();
 const org=await page.getByLabel("Tổ chức",{exact:true}).inputValue(),pid=await page.getByLabel("Dự án",{exact:true}).inputValue(),root="/api/v1/organizations/"+org+"/projects/"+pid;
 const data=await page.evaluate(async root=>{const req=async path=>{const r=await fetch(root+path);if(!r.ok)throw Error("API "+r.status);return r.json()};const [l,a,r,b]=await Promise.all([req("/layers"),req("/modules/house-numbering/areas"),req("/modules/house-numbering/routes"),req("/modules/house-numbering/buildings")]);return {layers:l.items.map(x=>({name:x.name,count:x.source.feature_count})),areas:a.items.filter(x=>x.name.includes("mẫu")).length,routes:r.items.filter(x=>x.source_label==="ovg-studio-sample-v1").length,buildings:b.items.length}},root);
 console.log(JSON.stringify(data));
 assert.equal(data.layers.length,5);assert.equal(data.areas,3);assert.equal(data.routes,4);assert.equal(data.buildings,13);assert.equal(errors.length,0,errors.join("\n"));await page.screenshot({path:"/tmp/ovg-sample-import.png"});console.log("PASS: sample import plus linked registry and map.");
}finally{await browser.close()}
