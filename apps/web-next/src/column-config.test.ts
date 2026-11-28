import {describe,it,expect} from "vitest";
import {readColumnConfig,moveColumn,completeSelection} from "./table-columns";
describe("personal attribute columns",()=>{
 it("migrates hidden-only preferences and appends new schema fields",()=>{
  expect(readColumnConfig('["old","b"]',["a","b"]).hidden).toEqual(["b"]);
  const c=readColumnConfig('{"hidden":["old"],"order":["b","b","old"],"widths":{"b":999,"a":2,"old":100}}',["a","b","c"]);
  expect(c.order).toEqual(["b","a","c"]);expect(c.widths).toEqual({b:600,a:70});
  expect(moveColumn(c.order,"b",1)).toEqual(["a","b","c"]);
 });
 it("never silently copies an incomplete selection",()=>{
  const rows=[{key:"a",layer:{id:"l"}},{key:"b",layer:{id:"l2"}}];
  expect(completeSelection(rows,new Set(["a","b"]),"l")).toBeNull();
  expect(completeSelection(rows,new Set(["a","c"]),"")).toBeNull();
  expect(completeSelection([...rows,rows[0]!],new Set(["a"]),"l")).toEqual([rows[0]]);
 });
});
