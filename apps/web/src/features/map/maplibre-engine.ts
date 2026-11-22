import {
  Map as LibreMap,
  setWorkerCount,
  setWorkerUrl,
  type GeoJSONSourceSpecification,
  type MapMouseEvent
} from "maplibre-gl";
import "maplibre-gl/dist/maplibre-gl.css";
import workerUrl from "maplibre-gl/dist/maplibre-gl-worker.mjs?url";
import type { MapViewpoint } from "@openvietgis/client-sdk";
import type { BasemapId, MapEngine, MapEngineEvents, RenderLayer } from "./engine";
import { boundsOfGeometry, type Bounds } from "./workspace";

setWorkerUrl(workerUrl);
setWorkerCount(1);

const LAT = 85.05112878;
const gridFeatures: Array<{
  type: "Feature";
  properties: Record<string, never>;
  geometry: { type: "LineString"; coordinates: number[][] };
}> = [];
for (let longitude = -180; longitude <= 180; longitude += 30) {
  gridFeatures.push({
    type: "Feature",
    properties: {},
    geometry: {
      type: "LineString",
      coordinates: [
        [longitude, -LAT],
        [longitude, LAT]
      ]
    }
  });
}
for (let latitude = -60; latitude <= 60; latitude += 30) {
  gridFeatures.push({
    type: "Feature",
    properties: {},
    geometry: {
      type: "LineString",
      coordinates: [
        [-180, latitude],
        [180, latitude]
      ]
    }
  });
}

const RASTER_LAYERS: Record<
  Exclude<BasemapId, "grid">,
  Array<{ id: string; tiles: string[]; maxzoom: number; attribution: string }>
> = {
  osm: [
    {
      id: "bm-osm",
      tiles: ["https://tile.openstreetmap.org/{z}/{x}/{y}.png"],
      maxzoom: 19,
      attribution: "© OpenStreetMap contributors"
    }
  ],
  satellite: [
    {
      id: "bm-sat",
      tiles: [
        "https://server.arcgisonline.com/ArcGIS/rest/services/World_Imagery/MapServer/tile/{z}/{y}/{x}"
      ],
      maxzoom: 19,
      attribution: "© Esri World Imagery"
    }
  ],
  hybrid: [
    {
      id: "bm-sat",
      tiles: [
        "https://server.arcgisonline.com/ArcGIS/rest/services/World_Imagery/MapServer/tile/{z}/{y}/{x}"
      ],
      maxzoom: 19,
      attribution: "© Esri World Imagery"
    },
    {
      id: "bm-labels",
      tiles: [
        "https://a.basemaps.cartocdn.com/rastertiles/voyager_only_labels/{z}/{x}/{y}.png",
        "https://b.basemaps.cartocdn.com/rastertiles/voyager_only_labels/{z}/{x}/{y}.png"
      ],
      maxzoom: 19,
      attribution: "© CARTO"
    }
  ],
  "google-satellite": [
    {
      id: "bm-google-satellite",
      tiles: ["https://mt0.google.com/vt/lyrs=s&x={x}&y={y}&z={z}"],
      maxzoom: 20,
      attribution: "© Google"
    }
  ],
  "google-hybrid": [
    {
      id: "bm-google-hybrid",
      tiles: ["https://mt0.google.com/vt/lyrs=y&x={x}&y={y}&z={z}"],
      maxzoom: 20,
      attribution: "© Google"
    }
  ],
  topo: [
    {
      id: "bm-topo",
      tiles: ["https://a.tile.opentopomap.org/{z}/{x}/{y}.png"],
      maxzoom: 17,
      attribution: "© OpenTopoMap"
    }
  ],
  dark: [
    {
      id: "bm-dark",
      tiles: [
        "https://a.basemaps.cartocdn.com/dark_all/{z}/{x}/{y}.png",
        "https://b.basemaps.cartocdn.com/dark_all/{z}/{x}/{y}.png"
      ],
      maxzoom: 19,
      attribution: "© CARTO"
    }
  ]
};

/** The only renderer-specific boundary. No attributes or physical storage bindings enter it. */
export function createMapLibreEngine(container: HTMLElement, events: MapEngineEvents): MapEngine {
  const map = new LibreMap({
    container,
    attributionControl: false,
    renderWorldCopies: false,
    maxPitch: 60,
    fadeDuration: 0,
    center: [105.84, 21.03],
    zoom: 5,
    style: {
      version: 8,
      sources: {
        "local-grid": {
          type: "geojson",
          data: { type: "FeatureCollection", features: gridFeatures }
        }
      },
      layers: [
        { id: "local-background", type: "background", paint: { "background-color": "#eef3f8" } },
        {
          id: "local-grid",
          type: "line",
          source: "local-grid",
          paint: { "line-color": "#cbd5e1", "line-width": 1 }
        }
      ]
    }
  });

  let layers: RenderLayer[] = [];
  let active: string | null = null;
  let selectedLayer: string | null = null;
  let selected: string[] = [];
  let currentBasemap: BasemapId = "grid";
  let loaded = false;
  let destroyed = false;
  let handles: { layer: string; source: string; logical: string }[] = [];
  let basemapHandles: string[] = [];

  function camera(): MapViewpoint {
    const center = map.getCenter();
    return {
      longitude: Math.max(-180, Math.min(180, center.lng)),
      latitude: Math.max(-LAT, Math.min(LAT, center.lat)),
      zoom: map.getZoom(),
      bearing: map.getBearing(),
      pitch: map.getPitch(),
      crs: 4326
    };
  }

  function settle(): void {
    if (destroyed) return;
    const b = map.getBounds();
    const west = Math.max(-180, b.getWest());
    const east = Math.min(180, b.getEast());
    const south = Math.max(-LAT, b.getSouth());
    const north = Math.min(LAT, b.getNorth());
    if (west < east && south < north) events.viewport([west, south, east, north], camera());
  }

  function selectedColor(logical: string, color: string): unknown {
    return selectedLayer === logical && selected.length
      ? ["case", ["in", ["get", "feature_id"], ["literal", selected]], "#f59e0b", color]
      : color;
  }

  function applyBasemap(): void {
    if (!loaded || destroyed) return;
    for (const id of basemapHandles) {
      if (map.getLayer(id)) map.removeLayer(id);
      if (map.getSource(id)) map.removeSource(id);
    }
    basemapHandles = [];

    if (map.getLayer("local-grid")) {
      map.setLayoutProperty(
        "local-grid",
        "visibility",
        currentBasemap === "grid" ? "visible" : "none"
      );
    }
    if (currentBasemap === "grid") {
      map.triggerRepaint();
      return;
    }

    const defs = RASTER_LAYERS[currentBasemap];
    const beforeId = handles[0]?.layer;
    for (const def of defs) {
      map.addSource(def.id, {
        type: "raster",
        tiles: def.tiles,
        tileSize: 256,
        maxzoom: def.maxzoom,
        attribution: def.attribution
      });
      map.addLayer({ id: def.id, type: "raster", source: def.id }, beforeId);
      basemapHandles.push(def.id);
    }
    map.triggerRepaint();
  }

  function apply(): void {
    if (!loaded || destroyed) return;
    if (!layers.length && !handles.length) return;
    events.ready(false);
    for (const h of handles) {
      if (map.getLayer(h.layer)) map.removeLayer(h.layer);
      if (map.getSource(h.source)) map.removeSource(h.source);
    }
    handles = [];
    // Arrangement is top-to-bottom in the tree; MapLibre adds topmost last.
    [...layers].reverse().forEach((layer, index) => {
      const source = "project-data-" + index;
      const features = layer.features.map((item) => {
        boundsOfGeometry(item.geometry);
        return {
          type: "Feature",
          id: item.feature_id,
          properties: { feature_id: item.feature_id },
          geometry: item.geometry
        };
      });
      map.addSource(source, {
        type: "geojson",
        data: { type: "FeatureCollection", features } as GeoJSONSourceSpecification["data"]
      });
      const family = layer.features[0]?.geometry.type;
      const handle = "project-layer-" + index;
      if (family === "Point") {
        map.addLayer({
          id: handle,
          source,
          type: "circle",
          paint: {
            "circle-color": selectedColor(layer.id, layer.style.color) as string,
            "circle-radius": layer.style.radius,
            "circle-opacity": layer.style.opacity,
            "circle-stroke-color": "#ffffff",
            "circle-stroke-width": 2
          }
        });
      } else if (family === "LineString") {
        map.addLayer({
          id: handle,
          source,
          type: "line",
          paint: {
            "line-color": selectedColor(layer.id, layer.style.color) as string,
            "line-width": layer.style.width,
            "line-opacity": layer.style.opacity
          }
        });
      } else if (family === "Polygon") {
        map.addLayer({
          id: handle,
          source,
          type: "fill",
          paint: {
            "fill-color": selectedColor(layer.id, layer.style.color) as string,
            "fill-opacity": layer.style.opacity,
            "fill-outline-color": "#1e293b"
          }
        });
      } else {
        map.removeSource(source);
        return;
      }
      handles.push({ layer: handle, source, logical: layer.id });
    });
    map.triggerRepaint();
  }

  function pick(event: MapMouseEvent): void {
    const target = handles.find((h) => h.logical === active);
    if (!target) return;
    const ids = map
      .queryRenderedFeatures(event.point, { layers: [target.layer] })
      .map((f) => f.properties?.feature_id)
      .filter((id): id is string => typeof id === "string")
      .sort();
    if (ids[0]) {
      events.pick(
        ids[0],
        event.originalEvent.shiftKey || event.originalEvent.ctrlKey || event.originalEvent.metaKey
      );
    }
  }

  map.on("load", () => {
    if (destroyed) return;
    loaded = true;
    applyBasemap();
    apply();
    settle();
  });
  map.on("click", pick);
  map.on("moveend", settle);
  map.on("idle", () => {
    if (!destroyed) events.ready(true);
  });
  map.on("error", () => {
    if (!destroyed) {
      // Only signal engine error for vector errors; ignore transient external tile network errors
      if (currentBasemap === "grid") {
        events.ready(false);
        events.error();
      }
    }
  });
  const observer = new ResizeObserver(() => {
    if (!destroyed) map.resize();
  });
  observer.observe(container);

  return {
    setBasemap(id) {
      currentBasemap = id;
      applyBasemap();
    },
    setLayers(value) {
      layers = value;
      apply();
    },
    setActiveLayer(value) {
      active = value;
    },
    setSelection(layerId, ids) {
      const hadSelection = selected.length > 0;
      if (selectedLayer === layerId && selected.join("\u0000") === ids.join("\u0000")) return;
      selectedLayer = layerId;
      selected = ids;
      if (hadSelection || ids.length > 0) apply();
    },
    setCamera(value) {
      map.jumpTo({
        center: [value.longitude, value.latitude],
        zoom: value.zoom,
        bearing: value.bearing,
        pitch: value.pitch
      });
      settle();
    },
    getCamera: camera,
    fitBounds(value: Bounds) {
      const [w, s, e, n] = value;
      map.fitBounds(
        [
          [w, s],
          [e, n]
        ],
        { padding: 48, maxZoom: 18, duration: 0 }
      );
    },
    zoom(delta) {
      map.zoomTo(Math.max(0, Math.min(22, map.getZoom() + delta)), { duration: 0 });
    },
    clear() {
      layers = [];
      active = null;
      selectedLayer = null;
      selected = [];
      apply();
    },
    destroy() {
      if (destroyed) return;
      destroyed = true;
      observer.disconnect();
      map.remove();
    }
  };
}
