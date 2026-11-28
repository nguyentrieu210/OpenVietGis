import {describe,it,expect} from "vitest";
import {enforceExclusive,changeVisibility,groupVisibilityState,remapExclusivePaths,cleanExclusivePaths} from "./layer-visibility";
const policy={layers:[{id:"a",group:"A"},{id:"b",group:"A/B"},{id:"c",group:"A/B"},{id:"d",group:"Other"},{id:"e"}],exclusive:["A","A/B"]};
describe("exclusive layer visibility",()=>{
 it("enforces nested groups and prefers explicitly shown layer",()=>{
  expect(enforceExclusive(new Set(),policy)).toEqual(new Set(["b","c"]));
  expect(changeVisibility(new Set(["b","c"]),policy,["c"],[])).toEqual(new Set(["a","b"]));
  expect(enforceExclusive(new Set(["a","b","c"]),policy)).toEqual(new Set(["a","b","c"]));
 });
 it("toggles exclusive group off and on without showing all children",()=>{
  const a=groupVisibilityState(["a","b","c"],new Set(["b","c"]),policy);
  expect(a.checked).toBe(true);expect(a.hide).toEqual(["a","b","c"]);
  const b=groupVisibilityState(["a","b","c"],new Set(["a","b","c"]),policy);
  expect(b.checked).toBe(false);expect(b.show).toEqual(["a"]);
 });
 it("handles nested subgroup and rename",()=>{
  const p=groupVisibilityState(["a","b","c"],new Set(["a","c"]),{...policy,exclusive:["A/B"]});
  expect(p.mixed).toBe(true);expect(p.show).toEqual(["a","b"]);expect(p.hide).toEqual(["c"]);
  expect(remapExclusivePaths(["A","A/B","Other"],"A","New")).toEqual(["New","New/B","Other"]);
  expect(remapExclusivePaths(["A","A/B"],"A",null)).toEqual(["B"]);
  expect(remapExclusivePaths(["A/B","A/B/C"],"A/B","A",false)).toEqual(["A/C"]);
  expect(cleanExclusivePaths(["A","A","A//B",1])).toEqual(["A"]);
 });
});
