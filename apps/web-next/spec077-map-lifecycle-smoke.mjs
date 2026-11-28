import {chromium} from "@playwright/test";
import assert from "node:assert/strict";
const browser=await chromium.launch({headless:true,executablePath:process.env.OVG_CHROMIUM_PATH||undefined,args:["--no-sandbox","--disable-dev-shm-usage","--use-angle=swiftshader","--enable-unsafe-swiftshader"]});
try{
 const page=await browser.newPage({viewport:{width:1440,height:900}});page.setDefaultTimeout(20000);
 await page.goto(process.env.OVG_MAP_FIXTURE_URL||"http://127.0.0.1:4199/spec077-map-fixture.html");
 const map=page.getByTestId("real-map"),set=map.getByRole("button",{name:"Set map state"});
 await set.click();const expected={x:106.6,y:11.1,zoom:12,selected:true},mount=await map.getAttribute("data-mount");
 assert.deepEqual(JSON.parse(await map.getByTestId("map-state").innerText()),expected);
 const panel=page.getByLabel("Panel",{exact:true});
 await panel.selectOption("layers");await page.getByRole("button",{name:"Dock tab",exact:true}).click();
 await panel.selectOption("map");await page.getByRole("button",{name:"Open panel"}).click();
 await panel.selectOption("layers");await page.getByRole("button",{name:"Dock right"}).click();await page.getByRole("button",{name:"Float panel"}).click();await page.getByRole("button",{name:"Dock left"}).click();
 await map.getByRole("button",{name:"Read map state"}).click();
 assert.equal(await map.getAttribute("data-mount"),mount);
 assert.deepEqual(JSON.parse(await map.getByTestId("map-state").innerText()),expected);
 assert.ok(await map.locator("canvas.maplibregl-canvas").count()===1);
 console.log("PASS real MapLibre renderer retained camera and selected feature state through docking");
}finally{await browser.close()}