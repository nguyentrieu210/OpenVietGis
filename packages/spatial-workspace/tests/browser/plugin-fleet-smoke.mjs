import {chromium} from "@playwright/test";
import assert from "node:assert/strict";
const browser=await chromium.launch({headless:true,args:["--no-sandbox"]});
try{
 const page=await browser.newPage();const errors=[];page.on("pageerror",e=>errors.push(e.message));
 await page.goto(process.env.SPATIAL_FIXTURE_URL??"http://127.0.0.1:5199/tests/browser/plugin-fleet.html");
 const button=name=>page.getByRole("button",{name,exact:true});
 await button("alpha count 0").click();await button("alpha count 1").waitFor();
 await page.evaluate(()=>{window.__map=document.querySelector('[data-testid="map"]')});
 await button("Beta").click();await button("beta count 0").click();await button("beta count 1").waitFor();
 await button("Background map call").click();assert.equal(await page.getByTestId("selected").textContent(),"beta");
 await button("Alpha").click();await button("alpha count 1").waitFor();
 await button("Background action").click();await page.waitForFunction(()=>document.querySelector('[data-testid="outcome"]')?.textContent==="denied");
 await button("Background map call").click();assert.equal(await page.getByTestId("selected").textContent(),"beta");
 await button("Core").click();assert.equal(await page.getByTestId("owner").textContent(),"core");
 await button("Beta").click();await button("beta count 1").waitFor();
 await button("Crash plugin").click();await page.getByRole("alert").getByText("fixture failure").waitFor();
 await button("Alpha").click();await button("alpha count 1").waitFor();
 await button("Remove beta").click();await button("Add beta").click();await button("Beta").click();await button("beta count 0").waitFor();
 assert.ok(await page.evaluate(()=>window.__map===document.querySelector('[data-testid="map"]')));
 assert.deepEqual(errors,[]);console.log("PASS two resident plugins retain independent state, foreground-only map/actions, isolated error, removal resets module state, Core DOM stable");
}finally{await browser.close()}
