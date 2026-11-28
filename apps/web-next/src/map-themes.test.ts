import {describe,it,expect} from "vitest";
import {snapshotTheme,applyThemeSnapshot,themeNameAllowed,MAP_THEME_LAYER_LIMIT} from "./map-themes";
import {emptyLayerView} from "./LayerProperties";
const l={id:"a",source_id:"s",lifecycle:"active",source:{schema_revision:"v1"},capabilities:[{name:"feature_read",available:true,actor_authorized:true}]};
describe("personal map themes",()=>{
 it("preserves current edit locks, new layers and changed schemas",()=>{
  const t=snapshotTheme([l],new Set(["a"]),{a:{...emptyLayerView,locked:false,opacity:.4}},emptyLayerView);
  const current={a:{...emptyLayerView,locked:true,selectable:false}};
  const p=applyThemeSnapshot(t,[l,{...l,id:"new"}],new Set(),current);
  expect(p.hide).toEqual(["a"]);expect(p.skipped).toBe(1);expect(p.views.a?.locked).toBe(true);expect(p.views.a?.selectable).toBe(false);expect(p.views.a?.opacity).toBe(.4);
  expect(applyThemeSnapshot(t,[{...l,source:{schema_revision:"v2"}}],new Set(),current).hide).toEqual([]);
  const defaults=snapshotTheme([l],new Set(),{},emptyLayerView);expect(applyThemeSnapshot(defaults,[l],new Set(),{a:{...emptyLayerView,opacity:.2}}).views.a?.opacity).toBe(1);
 });
 it("rejects legacy unpinned themes and reserved or duplicate names",()=>{
  expect(applyThemeSnapshot({hidden:["a"],views:{}},[l],new Set(),{}).hide).toEqual([]);
  expect(themeNameAllowed("__proto__",{})).toBe(false);expect(themeNameAllowed("A",{A:{}})).toBe(false);expect(themeNameAllowed("A",{})).toBe(true);
 });
});

it("rejects ambiguous names and never silently truncates theme membership",()=>{
 expect(themeNameAllowed(" A ",{})).toBe(false);
 expect(themeNameAllowed("A\nB",{})).toBe(false);
 expect(()=>snapshotTheme([l,l],new Set(),{},emptyLayerView)).toThrow();
 const oversized=Array.from({length:MAP_THEME_LAYER_LIMIT+1},(_,i)=>({...l,id:"l"+i}));
 expect(()=>snapshotTheme(oversized,new Set(),{},emptyLayerView)).toThrow();
 const valid=snapshotTheme(oversized.slice(0,MAP_THEME_LAYER_LIMIT),new Set(),{},emptyLayerView);
 expect(Object.keys(valid.pins??{})).toHaveLength(MAP_THEME_LAYER_LIMIT);
});
