import {describe,it,expect} from "vitest";
import {readHiddenColumns,togglePageSelection,toggleFeatureSelection,tableTsv} from "./table-columns";
describe("table column and selection preferences",()=>{
 it("ignores stale and invalid fields",()=>expect(readHiddenColumns('["b","z","b",4]',["a","b"])).toEqual(["b"]));
 it("toggles a page without affecting other pages",()=>{
  expect([...togglePageSelection(new Set(["outside","a"]),["a","b"],false)]).toEqual(["outside"]);
  expect([...toggleFeatureSelection(new Set(["a","b"]),"a")]).toEqual(["b"]);
 });
 it("copies only requested columns and guards spreadsheet formula strings",()=>{
  expect(tableTsv(["Name","Area"],[["=1+1",-4],["a\tb",3]])).toBe("Name\tArea\n'=1+1\t-4\na b\t3");
 });
});

import {compareTableValue,selectionScopePin} from "./table-columns";
describe("typed sort and revision scope",()=>{
 it("preserves precision and NULLS LAST in both directions",()=>{
  expect(compareTableValue("12345678901234567890.001","12345678901234567890.002","decimal","asc")).toBeLessThan(0);
  expect(compareTableValue("-0.02","-0.01","decimal","asc")).toBeLessThan(0);
  expect(compareTableValue("2","10","integer","asc")).toBeLessThan(0);
  expect(compareTableValue("1.5","2","integer","asc")).toBeGreaterThan(0);
  expect(compareTableValue("2","1.5","integer","asc")).toBeLessThan(0);
  expect(compareTableValue(null,1,"integer","desc")).toBeGreaterThan(0);
  expect(compareTableValue(false,true,"boolean","asc")).toBeLessThan(0);
 });
 it("pins content, schema and capability rather than array ordering",()=>{
  const a={id:"a",source_id:"s",lifecycle:"active",layer_revision:1,source:{content_revision:"a",schema_revision:"b"},capabilities:[{name:"feature_read",available:true,actor_authorized:true}]};
  const b={...a,id:"b"};
  expect(selectionScopePin([a,b],["read","edit"],false)).toBe(selectionScopePin([b,a],["edit","read"],false));
  expect(selectionScopePin([a],[],false)).not.toBe(selectionScopePin([{...a,source:{...a.source,content_revision:"next"}}],[],false));
  expect(selectionScopePin([a],[],false)).not.toBe(selectionScopePin([{...a,capabilities:[]}],[],false));
 });
});
