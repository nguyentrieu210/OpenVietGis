import * as maplibregl from "maplibre-gl";
import workerUrl from "maplibre-gl/dist/maplibre-gl-worker.mjs?url";
maplibregl.setWorkerUrl(workerUrl);
import "maplibre-gl/dist/maplibre-gl.css";
import {createDraft} from "./draft-contract.mjs";
import {TerraDraw, TerraDrawPointMode, TerraDrawLineStringMode, TerraDrawPolygonMode, TerraDrawSelectMode} from "terra-draw";
import {TerraDrawMapLibreGLAdapter} from "terra-draw-maplibre-gl-adapter";
// Synthetic local draft only: no project IDs, network services or persistence.
const map = new maplibregl.Map({
  container: "map", center: [106.9, 11.5], zoom: 12, attributionControl: false,
  style: {version: 8, sources: {}, layers: [{id: "background", type: "background", paint: {"background-color": "#eef1ec"}}]},
});
map.once("load", () => {
  const draw = new TerraDraw({
    adapter: new TerraDrawMapLibreGLAdapter({map, coordinatePrecision: 9, prefixId: "r031-poc"}),
    modes: [new TerraDrawPointMode(), new TerraDrawLineStringMode(), new TerraDrawPolygonMode(), new TerraDrawSelectMode()],
  });
  draw.start();
  draw.setMode("point");
  window.poc = {map, draw, createDraft};
});
