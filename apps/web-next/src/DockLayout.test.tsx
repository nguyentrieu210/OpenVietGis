import {describe,it,expect} from "vitest";
import {defaultDockLayout,dockLayoutPresets,getDockLayoutPreset,cleanDockLayout} from "./DockLayout";

describe("dock layout presets",()=>{
 it("identifies every built-in preset",()=>{
  for(const [id,layout] of Object.entries(dockLayoutPresets))expect(getDockLayoutPreset(layout)).toBe(id);
 });
 it("normalizes preset values and recognizes saved state",()=>{
  expect(getDockLayoutPreset(cleanDockLayout(dockLayoutPresets.analysis))).toBe("analysis");
  expect(getDockLayoutPreset(cleanDockLayout({...defaultDockLayout,icons:"large"}))).toBe("custom");
 });
});
