/**
 * 3D spatial workspace renderer adapter.
 * Owns direct renderer runtime imports, CSS and Mercator projection hooks
 * so tiles3d and mesh-layer stay renderer-agnostic.
 */
import { MercatorCoordinate } from "maplibre-gl";
import type { CustomLayerInterface, Map as Renderer3dMap } from "maplibre-gl";
import "maplibre-gl/dist/maplibre-gl.css";
import type { PreparedEngine } from "./coordinator";

export { MercatorCoordinate };
export type { CustomLayerInterface, Renderer3dMap };

export type GeographicCamera = Readonly<{
  longitude: number;
  latitude: number;
  zoom: number;
  bearing: number;
  pitch?: number;
}>;

export type SelectableFootprintEngine = PreparedEngine & {
  select(featureId: string): void;
  selectModel(id: string): void;
  setLayerVisible(id: string, visible: boolean): void;
  getCamera(): GeographicCamera;
};

export async function load3dMapRuntime() {
  const { Map, NavigationControl } = await import("maplibre-gl");
  return { Map, NavigationControl };
}
