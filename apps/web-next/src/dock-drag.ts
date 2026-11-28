import type { DockLayout } from "./DockLayout";

export type DockPanelId = "layers" | "inspector" | "table";
export type DockTarget = "left" | "right" | "bottom" | "float";
export type DockPlacementPatch =
  | Pick<DockLayout, "layers">
  | Pick<DockLayout, "inspector">
  | Pick<DockLayout, "table">;

export function dockPanelPatch(panel: DockPanelId, target: DockTarget): DockPlacementPatch | null {
  if (panel === "layers" && (target === "left" || target === "right")) return { layers: target };
  if (panel === "inspector" && (target === "left" || target === "right" || target === "float")) return { inspector: target };
  if (panel === "table" && (target === "bottom" || target === "right" || target === "float")) return { table: target };
  return null;
}

export function dockTargetsFor(panel: DockPanelId): DockTarget[] {
  if (panel === "layers") return ["left", "right"];
  if (panel === "inspector") return ["left", "right", "float"];
  return ["bottom", "right", "float"];
}

type AnchorRect = { left: number; top: number; bottom: number };
type Size = { width: number; height: number };
type ViewportSize = { width: number; height: number };
export type FittedDockMenu = { left: number; top: number; maxWidth: number; maxHeight: number };

/** Clamp the portal menu to the viewport, flipping above its trigger when needed. */
export function fitDockMenuPosition(anchor: AnchorRect, popup: Size, viewport: ViewportSize, gap = 4): FittedDockMenu {
  const width = Math.max(0, Number.isFinite(viewport.width) ? viewport.width : 0);
  const height = Math.max(0, Number.isFinite(viewport.height) ? viewport.height : 0);
  const inset = Math.min(Math.max(0, Number.isFinite(gap) ? gap : 0), width / 2, height / 2);
  const availableWidth = Math.max(0, width - inset * 2);
  const availableHeight = Math.max(0, height - inset * 2);
  const popupWidth = Math.min(availableWidth, Math.max(0, Number.isFinite(popup.width) ? popup.width : 0));
  const popupHeight = Math.min(availableHeight, Math.max(0, Number.isFinite(popup.height) ? popup.height : 0));
  const left = Math.max(inset, Math.min(Number.isFinite(anchor.left) ? anchor.left : inset, width - inset - popupWidth));
  let top = (Number.isFinite(anchor.bottom) ? anchor.bottom : inset) + inset;
  if (top + popupHeight > height - inset) top = (Number.isFinite(anchor.top) ? anchor.top : inset) - popupHeight - inset;
  top = Math.max(inset, Math.min(top, height - inset - popupHeight));
  return {
    left,
    top,
    maxWidth: availableWidth,
    maxHeight: Math.max(0, height - top - inset)
  };
}
