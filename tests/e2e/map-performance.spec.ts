import { expect, test } from "@playwright/test";
import { execFileSync } from "node:child_process";
import { appendFile, mkdir, writeFile } from "node:fs/promises";
import type { Request, Response } from "@playwright/test";
import { cpus, platform, release, totalmem } from "node:os";
import type { MapViewport, SpatialFeatureDetail } from "../../packages/client-sdk/src";
import {
  createMapProject, expectUsableMap, mapSignIn, normalMapViewpoint,
  registerMapReferences, saveFixtureMap
} from "./map-fixtures";

test.use({ viewport: { width: 1280, height: 900 }, trace: "off", screenshot: "off", video: "off" });

function summarize(values: number[]) {
  const sorted = [...values].sort((a, b) => a - b);
  const quantile = (fraction: number) => sorted[Math.ceil(sorted.length * fraction) - 1]!;
  return { count: values.length, p50_ms: quantile(0.5), p95_ms: quantile(0.95), max_ms: sorted.at(-1)! };
}

test("measures 100 actual map-open and select-to-authorized-inspector trials on the 10000-feature Core profile", async ({
  page, browser
}, testInfo) => {
  test.setTimeout(600_000);
  const warmups = 5, trials = 100;
  // Persist sanitized incremental diagnostics even if a later trial fails or the runner is interrupted.
  // No URLs, cookies, header values, request/response bodies or feature IDs are retained here.
  const phasePath = "dist/map-evidence/sc007-phases.jsonl";
  await mkdir("dist/map-evidence", { recursive: true });
  await writeFile(phasePath, JSON.stringify({
    schema_version: 1, event: "benchmark-start", exact_sha: execFileSync("git", ["rev-parse", "HEAD"], { encoding: "utf8" }).trim(),
    measured_at: new Date().toISOString(), warmups, trials
  }) + "\n");
  const recordPhase = (value: Record<string, unknown>) => appendFile(phasePath, JSON.stringify({ recorded_at: new Date().toISOString(), ...value }) + "\n");
  await mapSignIn(page);
  const fixture = await createMapProject(page, Date.now().toString() + "-benchmark");
  await registerMapReferences(page, fixture);
  expect(fixture.sources.reduce((sum, source) => sum + source.feature_count!, 0)).toBe(10000);
  await saveFixtureMap(page, fixture);
  await recordPhase({ event: "fixture-ready", measured_at: new Date().toISOString(), total_features: 10000 });
  const externalRequests: string[] = [];
  await page.route(/^https?:\/\/(?!(?:127\.0\.0\.1|localhost)(?::|\/|$))/i, async route => {
    externalRequests.push(new URL(route.request().url()).origin);
    await route.abort();
  });
  const opening: number[] = [];
  const selection: number[] = [];
  let observedViewport: Record<string, string> | null = null;
  let observedCanvas: { width: number; height: number } | null = null;

  for (let index = 0; index < warmups + trials; index++) {
    // Full fresh workspace document/renderer, retaining only normal warmed browser assets and session cookies.
    await page.goto("about:blank");
    await recordPhase({ event: "trial-start", workflow: "open", index, warmup: index < warmups });
    const snapshots: Promise<void>[] = [];
    const successfulLayers = new Set<string>();
    const viewportAttempts = new Map<Request, {
      layer: string; bbox_query_present: boolean; request_ms: number;
      response_ms: number | null; response_status: number | null; validated_ms: number | null; failed_ms: number | null;
    }>();
    let started = 0;
    let navigationMs: number | null = null, usableMs: number | null = null;
    let phase = "navigation", passed = false;
    const requestListener = (request: Request) => {
      const url = new URL(request.url());
      if (!url.pathname.startsWith(fixture.api + "/layers/") || !url.pathname.endsWith("/viewport")) return;
      const layer = fixture.layers.find(candidate => url.pathname === fixture.api + "/layers/" + candidate.id + "/viewport");
      viewportAttempts.set(request, {
        layer: layer?.name ?? "unexpected", bbox_query_present: ["west", "south", "east", "north"].every(key => url.searchParams.has(key)),
        request_ms: performance.now() - started, response_ms: null, response_status: null, validated_ms: null, failed_ms: null
      });
    };
    const requestFailedListener = (request: Request) => {
      const attempt = viewportAttempts.get(request);
      if (attempt) attempt.failed_ms = performance.now() - started;
    };
    const responseListener = (response: Response) => {
      const url = new URL(response.url());
      if (!url.pathname.startsWith(fixture.api + "/layers/") || !url.pathname.endsWith("/viewport")) return;
      const attempt = viewportAttempts.get(response.request());
      if (attempt) {
        attempt.response_ms = performance.now() - started;
        attempt.response_status = response.status();
      }
      snapshots.push((async () => {
        expect(response.status()).toBe(200);
        const body = await response.json() as MapViewport;
        const layer = fixture.layers.find(candidate => candidate.id === body.context.layer_id);
        expect(layer, "viewport must belong to this exact authorized Project fixture").toBeDefined();
        expect(body.context).toMatchObject({
          organization_id: fixture.organizationId, project_id: fixture.projectId,
          source_id: layer!.source_id, content_revision: layer!.source.content_revision,
          schema_revision: layer!.source.schema_revision, geometry_included: true,
          response_crs: { authority: "EPSG", code: 4326, coordinate_order: "xy" }
        });
        expect(body.complete).toBe(true);
        expect(body.limitation).toBeNull();
        expect(body.items.length).toBeGreaterThan(0);
        successfulLayers.add(layer!.id);
        if (attempt) attempt.validated_ms = performance.now() - started;
        if (!observedViewport) observedViewport = Object.fromEntries(url.searchParams.entries());
      })());
    };
    page.on("request", requestListener);
    page.on("requestfailed", requestFailedListener);
    page.on("response", responseListener);
    try {
      started = performance.now();
      await page.goto(fixture.root + "/spatial");
      navigationMs = performance.now() - started;
      phase = "renderer-and-data-ready";
      await expectUsableMap(page);
      usableMs = performance.now() - started;
      phase = "viewport-validation";
      await Promise.all(snapshots);
      expect(successfulLayers.size, "usable open must apply all three actual pinned geometry responses").toBe(3);
      const elapsed = performance.now() - started;
      if (index >= warmups) opening.push(elapsed);
      if (!observedCanvas) {
        const box = await page.getByTestId("map-canvas").boundingBox();
        expect(box).not.toBeNull();
        observedCanvas = { width: box!.width, height: box!.height };
      }
      passed = true;
    } finally {
      // Diagnostics are outside the timed measurement. Failed navigation/DOM checks cannot mask the original assertion.
      const finishedMs = performance.now() - started;
      page.off("request", requestListener);
      page.off("requestfailed", requestFailedListener);
      page.off("response", responseListener);
      const readyAttribute = passed ? "true" : await page.getByTestId("map-engine-ready").getAttribute("data-ready", { timeout: 250 }).catch(() => null);
      const statusAttribute = passed ? "usable" : await page.getByTestId("map-status").getAttribute("data-state", { timeout: 250 }).catch(() => null);
      await recordPhase({
        event: "trial-end", workflow: "open", index, warmup: index < warmups, passed,
        phase, elapsed_ms: finishedMs, navigation_ms: navigationMs, usable_ms: usableMs,
        engine_ready: readyAttribute === null ? null : readyAttribute === "true",
        map_status: statusAttribute, successful_layer_count: successfulLayers.size,
        viewport_attempts: [...viewportAttempts.values()]
      });
    }
  }

  const points = fixture.layers.find(layer => layer.name === "Points")!;
  await page.getByRole("button", { name: "Chọn Points làm lớp hiện hành", exact: true }).click();
  const table = page.getByRole("table", { name: "Bảng thuộc tính theo phiên bản" });
  await expect(table.getByRole("button", { name: "point-000001", exact: true })).toBeVisible();
  for (let index = 0; index < warmups + trials; index++) {
    await recordPhase({ event: "trial-start", workflow: "select", index, warmup: index < warmups });
    const id = index % 2 === 0 ? "point-000001" : "point-000002";
    const actualDetail = page.waitForResponse(response => {
      const url = new URL(response.url());
      return url.pathname === fixture.api + "/layers/" + points.id + "/features/" + id &&
        response.request().method() === "GET";
    });
    const started = performance.now();
    let phase = "selection", passed = false;
    let detailMs: number | null = null, detailStatus: number | null = null;
    try {
      await table.getByRole("button", { name: id, exact: true }).click();
      phase = "detail-response";
      const detailResponse = await actualDetail;
      detailMs = performance.now() - started;
      detailStatus = detailResponse.status();
      expect(detailResponse.status()).toBe(200);
      const detail = await detailResponse.json() as SpatialFeatureDetail;
      expect(detail.context).toMatchObject({
        organization_id: fixture.organizationId, project_id: fixture.projectId,
        layer_id: points.id, source_id: points.source_id,
        content_revision: points.source.content_revision, schema_revision: points.source.schema_revision
      });
      expect(detail.feature.feature_id).toBe(id);
      phase = "inspector-and-selection";
      await expect(page.getByTestId("map-inspector")).toHaveAttribute("data-feature-id", id);
      await expect(page.getByTestId("feature-select-" + id)).toBeChecked();
      const elapsed = performance.now() - started;
      if (index >= warmups) selection.push(elapsed);
      passed = true;
    } finally {
      await recordPhase({
        event: "trial-end", workflow: "select", index, warmup: index < warmups, passed,
        phase, elapsed_ms: performance.now() - started, detail_response_ms: detailMs, detail_status: detailStatus
      });
    }
  }

  const sourcePins = fixture.sources.map(source => ({
    source_id: source.id, content_revision: source.content_revision, schema_revision: source.schema_revision,
    manifest_digest: source.provenance.manifest_digest, feature_count: source.feature_count,
    geometry_family: source.schema.geometry_family, crs: source.stored_crs?.code
  }));
  const browserProfile = await page.evaluate(() => ({
    viewport: { width: window.innerWidth, height: window.innerHeight },
    device_pixel_ratio: window.devicePixelRatio, hardware_concurrency: navigator.hardwareConcurrency
  }));
  const report = {
    schema_version: 1, criterion: "R004 SC007", measured_at: new Date().toISOString(),
    exact_sha: execFileSync("git", ["rev-parse", "HEAD"], { encoding: "utf8" }).trim(),
    runner: process.env.GITHUB_RUN_ID ? {
      provider: "GitHub Actions", run_id: process.env.GITHUB_RUN_ID,
      runner_name: process.env.RUNNER_NAME, runner_os: process.env.RUNNER_OS
    } : { provider: "local", runner_os: platform() },
    runtime: { node: process.version, browser: browser.version(), platform: platform(), release: release(),
      cpu_model: cpus()[0]?.model, cpu_count: cpus().length, memory_bytes: totalmem() },
    browser_profile: browserProfile, canvas: observedCanvas, designated_viewpoint: normalMapViewpoint,
    observed_viewport_query: observedViewport, source_pins: sourcePins, total_features: 10000,
    external_network: "blocked", external_request_count: externalRequests.length,
    warmup_trials_per_workflow: warmups, measured_trials_per_workflow: trials,
    open: { ...summarize(opening), budget_p95_ms: 3000, raw_ms: opening },
    select: { ...summarize(selection), budget_p95_ms: 1000, raw_ms: selection }
  };
  // Sanitized synthetic measurements only; no cookies, tokens, bodies, identity credentials or traces.
  await mkdir("dist/map-evidence", { recursive: true });
  await writeFile("dist/map-evidence/sc007.json", JSON.stringify(report, null, 2) + "\n");
  await testInfo.attach("R004-SC007-measured-map-workflows", {
    body: JSON.stringify(report), contentType: "application/json"
  });
  await testInfo.attach("R004-SC007-phase-diagnostics", { path: phasePath, contentType: "application/x-ndjson" });
  expect(externalRequests).toEqual([]);
  expect(opening).toHaveLength(100);
  expect(selection).toHaveLength(100);
  expect(report.open.p95_ms).toBeLessThanOrEqual(3000);
  expect(report.select.p95_ms).toBeLessThanOrEqual(1000);
});
