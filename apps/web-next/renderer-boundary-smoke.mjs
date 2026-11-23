import { chromium } from "@playwright/test";
import assert from "node:assert/strict";

const url = process.env.OVG_TEST_URL;
assert.ok(url && new URL(url).hostname === "127.0.0.1", "Loopback fixture only");
const org = "fixture-org", pid = "fixture-project";
const project = {id:pid, organization_id:org, name:"Renderer fixture", status:"active"};
const browser = await chromium.launch({headless:true,args:["--no-sandbox","--use-angle=swiftshader","--enable-unsafe-swiftshader"]});
try {
  const page = await browser.newPage({viewport:{width:1440,height:900}});
  const errors = [];
  page.on("pageerror", e => errors.push(e.message));
  await page.route("**/api/v1/**", route => {
    const p = new URL(route.request().url()).pathname;
    let body;
    if (p.endsWith("/me")) body={id:"fixture",display_name:"Renderer Test",status:"active"};
    else if (p.endsWith("/auth/providers")) body=[{id:"keycloak",display_name:"Keycloak"}];
    else if (p.endsWith("/auth/csrf")) body={token:"fixture-csrf"};
    else if (p.endsWith("/me/organizations")) body=[{id:org,name:"Fixture",role:"owner",status:"active"}];
    else if (p.endsWith("/projects")) body=[project];
    else if (p.endsWith("/projects/"+pid)) body=project;
    else if (p.endsWith("/access/mine")) body={restricted:false,rules:[]};
    else if (p.endsWith("/effective-permissions")) body={permissions:["project.read","project.edit","project.manage"]};
    else if (p.endsWith("/map")) body={configuration_revision:1,configuration:{layers:[],basemap:"grid",default_viewpoint:null}};
    else if (p.endsWith("/avatar")) body={data_url:null};
    else if (p.endsWith("/layers")) body={items:[],next_after_id:null};
    else if (p.endsWith("/extensions/activations")||p.endsWith("/extensions/manifests")||p.endsWith("/extensions/partner-contributions")) body={items:[]};
    else return route.fulfill({status:404,json:{}});
    return route.fulfill({json:body});
  });
  await page.route("**/.well-known/oauth-protected-resource/mcp", r => r.fulfill({status:404,json:{}}));
  await page.route("https://**/*", r=>r.abort());
  await page.goto(url,{waitUntil:"domcontentloaded",timeout:20000});
  const canvas=page.locator(".qgis-map-region canvas").first();
  const command=page.getByLabel("Dòng lệnh GIS");
  await canvas.waitFor({timeout:20000});
  await command.waitFor({timeout:10000});
  assert.ok(await canvas.evaluate(el=>el.width>0&&el.height>0));
  console.log("PASS actual workspace canvas and command line initialized");
  await command.fill("DIST 106.7,10.8 106.701,10.8");
  await command.press("Enter");
  await page.locator(".core-sketch").waitFor({timeout:8000});
  await page.keyboard.press("Escape");
  await page.locator(".core-sketch").waitFor({state:"detached",timeout:8000});
  await canvas.click({button:"right",position:{x:100,y:100}});
  await page.locator(".core-sketch").waitFor({timeout:8000});
  await page.keyboard.press("Escape");
  assert.deepEqual(errors,[]);
  console.log("PASS canvas context command, measurement, escape and no JS errors");
} catch (error) {
  console.error(error);
  process.exitCode=1;
} finally {
  await browser.close();
}
