import {chromium} from "@playwright/test";import assert from "node:assert/strict";
const browser=await chromium.launch({headless:true,args:["--no-sandbox","--use-angle=swiftshader","--enable-unsafe-swiftshader"]});
try{
 const page=await browser.newPage({viewport:{width:1100,height:800}}),errors=[],external=[];
 page.on("pageerror",e=>errors.push(e.message));await page.route("**/*",r=>{if(new URL(r.request().url()).hostname!=="127.0.0.1"){external.push(r.request().url());return r.abort()}return r.continue()});
 await page.goto("http://127.0.0.1:5199/tiles.html");
 await page.locator("#status").filter({hasText:"1 tile · 12 tam giác"}).waitFor({timeout:30000});
 assert.match(await page.locator("#requests").textContent(),/coarse.glb/);assert.doesNotMatch(await page.locator("#requests").textContent(),/detail.glb/);
 const canvas=page.locator(".maplibregl-canvas"),png=await canvas.screenshot();
 const reds=await page.evaluate(async data=>{const image=await createImageBitmap(await(await fetch("data:image/png;base64,"+data)).blob());const c=document.createElement("canvas");c.width=image.width;c.height=image.height;const ctx=c.getContext("2d");ctx.drawImage(image,0,0);const a=ctx.getImageData(0,0,c.width,c.height).data;let n=0;for(let i=0;i<a.length;i+=4)if(a[i]>100&&a[i+1]<50&&a[i+2]<50)n++;return n},png.toString("base64"));assert.ok(reds>20,"visible pixels "+reds);
 for(let i=0;i<3;i++){await page.locator(".maplibregl-ctrl-zoom-in").click();await page.waitForTimeout(450)}
 await page.locator("#requests").filter({hasText:"detail.glb"}).waitFor({timeout:15000});
 await page.locator("#status").filter({hasText:"1 tile · 12 tam giác"}).waitFor();
 await page.getByRole("button",{name:"Dispose"}).click();assert.equal(await canvas.count(),0);assert.deepEqual(errors,[]);assert.deepEqual(external,[]);
 console.log("PASS coarse tile pixels, zoom REPLACE detail fetch, one resident tile, disposal, no external requests");
}finally{await browser.close()}
