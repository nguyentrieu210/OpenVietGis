import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { performance } from "node:perf_hooks";
import { chromium } from "@playwright/test";

const fixture = JSON.parse(readFileSync(0, "utf8"));
const useTiles = fixture.rows > 50_000;
const expectedPages = useTiles ? 0 : Math.ceil(fixture.rows / 1_000);
const rounds = Number(process.env.OVG059_BROWSER_RELOADS ?? 20);
assert.ok(Number.isInteger(rounds) && rounds > 0);
const args = [
  "--use-angle=swiftshader",
  "--enable-unsafe-swiftshader",
  "--host-resolver-rules=MAP tile.openstreetmap.org ~NOTFOUND",
];
const browser = await chromium.launch({ args });

try {
  const context = await browser.newContext({ viewport: { width: 1440, height: 900 } });
  await context.addCookies([{
    name: "ovg_session", value: fixture.token, url: fixture.origin, httpOnly: true,
  }]);
  await context.addInitScript(({ cameraKey }) => {
    if (!localStorage.getItem(cameraKey)) {
      localStorage.setItem(cameraKey, JSON.stringify({ center: [106.9, 11.5], zoom: 14 }));
    }
  }, { cameraKey: fixture.cameraKey });
  const page = await context.newPage();
  const cdp = await context.newCDPSession(page);
  const isTile = path => path.includes("/pro/tiles/" + fixture.layerId + "/");
  const isPage = path => path.endsWith("/layers/" + fixture.layerId + "/features");
  const isSpatial = path => isTile(path) || isPage(path);
  const pending = new Set();
  const finished = [];
  const failures = [];
  let canceledTiles = 0;
  page.on("request", request => {
    if (isSpatial(new URL(request.url()).pathname)) pending.add(request);
  });
  page.on("requestfinished", request => {
    if (pending.delete(request)) finished.push(request);
  });
  page.on("requestfailed", request => {
    if (!pending.delete(request)) return;
    const error = request.failure()?.errorText;
    if (error === "net::ERR_ABORTED" && isTile(new URL(request.url()).pathname)) {
      canceledTiles += 1;
    } else {
      failures.push(error);
    }
  });
  page.on("pageerror", error => failures.push(error.message));
  const samples = [];

  for (let i = 0; i <= rounds; i += 1) {
    finished.length = 0;
    canceledTiles = 0;
    const started = performance.now();
    if (i === 0) await page.goto(fixture.origin + "/app", { waitUntil: "domcontentloaded" });
    else await page.reload({ waitUntil: "domcontentloaded" });
    await page.locator(".maplibregl-canvas").waitFor({ timeout: 30_000 });
    if (useTiles) {
      await page.getByText(/đã tải · Tiles/).waitFor({ timeout: 30_000 });
    } else {
      const visibleCount = fixture.rows.toLocaleString("vi-VN");
      await page.locator(".qgis-footer-message").filter({
        hasText: visibleCount + " đang hiển thị",
      }).waitFor({ timeout: 30_000 });
    }
    await page.locator(".live-loading").waitFor({ state: "hidden" });
    const expires = performance.now() + 30_000;
    while ((pending.size > 0 || (useTiles && finished.length === 0)) &&
           performance.now() < expires) {
      await new Promise(resolve => setTimeout(resolve, 30));
    }
    assert.equal(pending.size, 0, "Map data requests must finish");
    await page.evaluate(() => new Promise(resolve =>
      requestAnimationFrame(() => requestAnimationFrame(resolve))));
    const ms = performance.now() - started;
    // Worker tile requests are absent from the window Resource Timing API.
    const requests = await Promise.all(finished.map(async request => ({
      path: new URL(request.url()).pathname,
      status: (await request.response())?.status(),
      ...await request.sizes(),
    })));
    const pages = requests.filter(request => isPage(request.path)).length;
    const tiles = requests.filter(request => isTile(request.path)).length;
    const transferKiB = requests
      .reduce((sum, request) => sum + request.responseHeadersSize + request.responseBodySize, 0)
      / 1024;
    assert.ok(requests.every(request => [200, 304].includes(request.status)));
    if (useTiles && i === 0) assert.ok(requests.some(request => request.responseBodySize > 0));
    const heap = await cdp.send("Runtime.getHeapUsage");
    assert.equal(pages, i === 0 ? expectedPages : 0);
    if (useTiles) assert.ok(tiles > 0);
    assert.deepEqual(failures, []);
    samples.push({ ms, pages, tiles, canceledTiles, transferKiB, jsHeapMiB: heap.usedSize / (1024 * 1024) });
  }

  const cold = samples[0];
  const warm = samples.slice(1);
  const percentile = (field, p) => warm.map(sample => sample[field]).sort((a, b) => a - b)
    [Math.ceil(warm.length * p) - 1];
  console.log(JSON.stringify({
    rows: fixture.rows, chromium: browser.version(), warmCount: warm.length, cold,
    warm: {
      p50Ms: percentile("ms", 0.5), p95Ms: percentile("ms", 0.95),
      maxPages: percentile("pages", 1), p95Tiles: percentile("tiles", 0.95),
      maxCanceledTiles: percentile("canceledTiles", 1),
      p95TransferKiB: percentile("transferKiB", 0.95), p95JsHeapMiB: percentile("jsHeapMiB", 0.95),
    },
  }));
} finally {
  await browser.close();
}
