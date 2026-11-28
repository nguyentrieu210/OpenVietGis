import {describe,it,expect} from "vitest";
import {joinKeyOf,joinLookup,stripTrailingNewlines} from "./join-contract";
describe("spreadsheet join contract",()=>{
 it("preserves identifiers and rejects empty, null and imprecise keys",()=>{
  expect(joinKeyOf("001")).toBe("001");expect(joinKeyOf(1)).toBe("1");
  expect(joinKeyOf(" ")).toBeNull();expect(joinKeyOf(null)).toBeNull();expect(joinKeyOf(9007199254740992)).toBeNull();
 });
 it("enforces unique lookup keys and preserves blank target cells",()=>{
  const r=joinLookup([["001",""],["","skip"],["002","  text  "]],0,1);
  expect(r.map.get("001")).toBe("");expect(r.map.get("002")).toBe("  text  ");expect(r.blank).toBe(1);
  expect(()=>joinLookup([["a","1"],["a","2"]],0,1)).toThrow();
  expect(()=>joinLookup([["a"]],0,0)).toThrow();
  expect(stripTrailingNewlines("key\tvalue\na\t\n")).toBe("key\tvalue\na\t");
 });
});
