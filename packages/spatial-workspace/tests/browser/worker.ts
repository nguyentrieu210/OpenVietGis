/**
 * Isolated browser test fixture adapter for renderer worker setup.
 * Never imported by production code.
 */
import { setWorkerUrl, type Map as BrowserTestMap } from "maplibre-gl";
import workerUrl from "maplibre-gl/dist/maplibre-gl-worker.mjs?url";

export { prepareFootprint3d } from "../../src/maplibre3d";
export type { BrowserTestMap };

export function configureBrowserTestWorker(): void {
  setWorkerUrl(workerUrl);
}
