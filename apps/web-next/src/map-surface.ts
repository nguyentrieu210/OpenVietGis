/**
 * Studio's renderer compatibility boundary.
 * Only this adapter owns renderer imports, worker/CSS bootstrap and DOM hooks.
 * The native-shaped surface types are transitional; do not expose them in
 * public GIS/plugin contracts (those use GeoJSON and studio-contract).
 */
import {
  Map, Marker, LngLat, LngLatBounds, NavigationControl, ScaleControl,
  setWorkerCount, setWorkerUrl,
} from "maplibre-gl";
import "maplibre-gl/dist/maplibre-gl.css";
import workerUrl from "maplibre-gl/dist/maplibre-gl-worker.mjs?url";

export { Map, Marker, LngLat, LngLatBounds, NavigationControl, ScaleControl };
export type {
  ErrorEvent, FillLayerSpecification, GeoJSONSource, LayerSpecification,
  LngLatBoundsLike, LngLatLike, MapMouseEvent, StyleSpecification,
} from "maplibre-gl";

export function configureMapSurface(): void {
  setWorkerUrl(workerUrl + "?mime=2");
  setWorkerCount(1);
}

/** The canvas DOM class is an implementation detail, not a command contract. */
export function isStudioMapCanvas(target: EventTarget | null): boolean {
  return target instanceof Element && !!target.closest(".qgis-map-region .maplibregl-canvas");
}
