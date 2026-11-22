import { chromium } from "@playwright/test";
import assert from "node:assert/strict";
const browser=await chromium.launch({headless:true,args:["--no-sandbox","--use-angle=swiftshader","--enable-unsafe-swiftshader"]});
try {
 const page=await browser.newPage({viewport:{width:1440,height:900}});
 const errors=[];page.on("pageerror",e=>errors.push(String(e)));page.on("console",m=>{if(m.type()==="error")console.log("console:",m.text().slice(0,300))});
 const response=await page.goto("https://vps1.gpcds.site:2087/rewrite/",{waitUntil:"networkidle",timeout:25000});
 assert.equal(response?.status(),200);
 await page.getByText("MapLibre sẵn sàng").waitFor({timeout:20000});
 await page.screenshot({path:"/tmp/ovg-next-desktop.png"});
 await page.getByRole("button",{name:"Đường Nguyễn Huệ"}).click();
 await page.getByRole("button",{name:"Bảng thuộc tính"}).click();
 await page.getByRole("button",{name:"100",exact:true}).click();
 await page.getByRole("heading",{name:"Số nhà 100"}).waitFor();
 await page.getByRole("button",{name:"Sửa tọa độ"}).click();
 await page.getByLabel("Kinh độ X").fill("106.727123");
 await page.getByRole("button",{name:"Áp dụng tọa độ"}).click();
 await page.getByText("106.727123°",{exact:true}).waitFor();
 await page.getByRole("button",{name:"Di chuyển",exact:true}).click();
 await page.locator(".drag-marker").waitFor();
 const marker=await page.locator(".drag-marker").boundingBox();
 if(marker){await page.mouse.move(marker.x+marker.width/2,marker.y+marker.height/2);await page.mouse.down();await page.mouse.move(marker.x+marker.width/2+45,marker.y+marker.height/2-20,{steps:10});await page.mouse.up();await page.getByText("Đã di chuyển điểm · Chưa lưu").waitFor();}
 await page.getByRole("button",{name:"Thêm dữ liệu",exact:true}).click();
 const fixture={type:"FeatureCollection",features:[{type:"Feature",geometry:{type:"Point",coordinates:[106.732,10.722]},properties:{name:"Test"}}]};
 await page.locator('input[type=file]').setInputFiles({name:"test.geojson",mimeType:"application/geo+json",buffer:Buffer.from(JSON.stringify(fixture))});
 await page.getByRole("button",{name:"Thêm vào bản đồ"}).click();
 await page.getByText("Đã thêm 1 đối tượng trên bản đồ").waitFor();
 await page.getByText("test.geojson",{exact:true}).waitFor();
 await page.screenshot({path:"/tmp/ovg-next-import.png"});
 assert.equal(errors.length,0,errors.join("\n"));
 console.log("PASS: live URL, MapLibre ready, area/route filter, table selection, coordinate update, marker drag, GeoJSON import; zero page errors");
} finally {await browser.close()}
