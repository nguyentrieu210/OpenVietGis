import {simulateSurfaceFlow, type HydraulicOptions} from "@openvietgis/spatial-workspace/terrain-analysis";
import type {TerrainGrid} from "@openvietgis/spatial-workspace/terrain";

self.onmessage = (event: MessageEvent<{grid: TerrainGrid; options: HydraulicOptions}>) => {
    try {
        const frames = simulateSurfaceFlow(event.data.grid, event.data.options);
        self.postMessage({frames}, {transfer: frames.map(frame => frame.depths.buffer)});
    } catch (err) {
        self.postMessage({error: err instanceof Error ? err.message : String(err)});
    }
};
