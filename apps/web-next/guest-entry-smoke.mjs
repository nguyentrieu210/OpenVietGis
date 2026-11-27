import {chromium} from "@playwright/test";
import assert from "node:assert/strict";
const base=process.env.GUEST_TEST_URL??"http://127.0.0.1:4179";
const browser=await chromium.launch({headless:true,args:["--no-sandbox","--use-angle=swiftshader","--enable-unsafe-swiftshader"]});
try{
 const page=await browser.newPage({viewport:{width:1440,height:900}});
 const calls=[];
 await page.route("**/api/v1/**",async route=>{calls.push(route.request().url());await route.fulfill({status:401,contentType:"application/json",body:"{}"})});
 await page.route("https://tiles.maps.eox.at/**",route=>route.abort());
 await page.goto(base);
 await page.getByRole("region",{name:"GIS dùng thử"}).waitFor({timeout:60000});
 await page.getByRole("button",{name:"Vẽ điểm"}).waitFor();
 await page.locator(".guest-tools input[type=file]").setInputFiles({name:"point.geojson",mimeType:"application/geo+json",buffer:Buffer.from(JSON.stringify({type:"Feature",geometry:{type:"Point",coordinates:[106,10]},properties:{}}))});
 await page.getByText("Đối tượng cục bộ (1)").waitFor();
 await page.reload();await page.getByText("Đối tượng cục bộ (1)").waitFor();
 const [download]=await Promise.all([page.waitForEvent("download"),page.getByRole("button",{name:"Xuất GeoJSON"}).click()]);
 assert.equal(download.suggestedFilename(),"openvietgis-guest.geojson");
 assert.equal(calls.some(url=>/\/organizations\//.test(url)),false);
 await page.getByRole("link",{name:"Đăng nhập",exact:true}).click();
 await page.getByRole("dialog",{name:"Tài khoản OpenVietGIS"}).waitFor();
 console.log("PASS: direct guest entry, local import, reload, export, no tenant API, optional login dialog");
}finally{await browser.close()}
