import { chromium } from "@playwright/test";
import { readFileSync } from "node:fs";
import assert from "node:assert/strict";
const fixture=readFileSync(new URL("./live-smoke.mjs",import.meta.url),"utf8");
const username=fixture.match(/locator\("#username"\)\.fill\("([^"]+)"\)/)?.[1];
const password=fixture.match(/locator\("#password"\)\.fill\("([^"]+)"\)/)?.[1];
assert.ok(username&&password,"isolated test fixture not configured");
const browser=await chromium.launch({headless:true,args:["--no-sandbox","--use-angle=swiftshader","--enable-unsafe-swiftshader"]});
try {
  const page=await browser.newPage({locale:"vi-VN"});
  await page.goto("https://gis.kairo.vn/");
  await page.getByRole("link",{name:/Đăng nhập với/}).click();
  await page.locator("#username").fill(username);
  await page.locator("#password").fill(password);
  await page.locator("#kc-login").click();
  await page.getByRole("button",{name:"Đăng xuất",exact:true}).waitFor({timeout:30000});
  await page.getByRole("button",{name:"Đăng xuất",exact:true}).click();
  await page.locator("#username").waitFor({state:"visible",timeout:30000});
  assert.equal(await page.locator("#username").isEditable(),true,"email field must be editable after logout");
  assert.equal(await page.locator("#username").inputValue(),"","email field must be blank");
  assert.equal(await page.locator("#password").isEditable(),true,"password field must be editable");
  assert.equal(await page.locator("#reset-login").count(),0,"previous identity must not be locked");
  console.log("PASS: logout from app with existing Keycloak session returns blank editable username/password");
} finally { await browser.close(); }
