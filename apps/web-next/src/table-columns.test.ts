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
