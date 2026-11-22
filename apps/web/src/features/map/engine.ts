import type { MapStyle, MapViewpoint } from "@openvietgis/client-sdk";
import type { Bounds } from "./workspace";

export type BasemapId = "grid" | "osm" | "satellite" | "hybrid" | "google-satellite" | "google-hybrid" | "topo" | "dark";

export interface BasemapOption {
  id: BasemapId;
  label: string;
}

export const BASEMAP_OPTIONS: BasemapOption[] = [
  { id: "google-hybrid", label: "Google Hybrid (Vệ tinh + Nhãn)" },
  { id: "google-satellite", label: "Vệ tinh Google" },
  { id: "hybrid", label: "Vệ tinh Esri + Nhãn" },
  { id: "satellite", label: "Vệ tinh Esri" },
  { id: "osm", label: "Đường phố (OSM)" },
  { id: "topo", label: "Địa hình (Topo)" },
  { id: "dark", label: "Nền tối" },
  { id: "grid", label: "Lưới tọa độ (Ngoại tuyến)" }
];

export interface RenderGeometry {
  type: "Point" | "LineString" | "Polygon";
  coordinates: unknown;
}
export interface RenderFeature {
  feature_id: string;
  geometry: RenderGeometry;
}
export interface RenderLayer {
  id: string;
  style: MapStyle;
  features: RenderFeature[];
}
export interface MapEngineEvents {
  viewport: (bounds: Bounds, camera: MapViewpoint) => void;
  pick: (id: string, multiple: boolean) => void;
  ready: (ready: boolean) => void;
  error: () => void;
}
export interface MapEngine {
  setBasemap(id: BasemapId): void;
  setLayers(layers: RenderLayer[]): void;
  setActiveLayer(id: string | null): void;
  setSelection(layerId: string | null, ids: string[]): void;
  setCamera(camera: MapViewpoint): void;
  getCamera(): MapViewpoint;
  fitBounds(bounds: Bounds): void;
  zoom(delta: number): void;
  clear(): void;
  destroy(): void;
}
