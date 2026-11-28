import {describe,it,expect} from "vitest";
import {dockPanelPatch,dockTargetsFor,fitDockMenuPosition} from "./dock-drag";

describe("dock drag targets",()=>{
 it("moves panels only to supported positions",()=>{
  expect(dockPanelPatch("layers","right")).toEqual({layers:"right"});
  expect(dockPanelPatch("inspector","float")).toEqual({inspector:"float"});
  expect(dockPanelPatch("table","bottom")).toEqual({table:"bottom"});
  expect(dockPanelPatch("layers","float")).toBeNull();
  expect(dockPanelPatch("table","left")).toBeNull();
  expect(dockTargetsFor("table")).toEqual(["bottom","right","float"]);
 });
 it("fits the dock menu inside a narrow viewport and flips above its trigger",()=>{
  const fitted=fitDockMenuPosition(
   {left:228,top:138,bottom:155},
   {width:320,height:280},
   {width:240,height:160}
  );
  expect(fitted.left).toBeGreaterThanOrEqual(4);
  expect(fitted.left+fitted.maxWidth).toBeLessThanOrEqual(236);
  expect(fitted.top).toBeGreaterThanOrEqual(4);
  expect(fitted.top+fitted.maxHeight).toBeLessThanOrEqual(156);
 });
});
