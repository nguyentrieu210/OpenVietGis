import {describe,it,expect} from "vitest";
import {mergeSourcePage} from "./source-browser";
const source=(id:string,lifecycle="active")=>({id,lifecycle});
describe("source browser pagination",()=>{
 it("appends active rows and retains a cursor even across retired-only pages",()=>{
  const a=mergeSourcePage([],{items:[source("a"),source("r","retired")],next_after_id:"r"},null,new Set());
  expect(a.items.map(x=>x.id)).toEqual(["a"]);expect(a.next).toBe("r");
  const b=mergeSourcePage(a.items,{items:[source("b")],next_after_id:null},"r",new Set(["r"]));expect(b.items.map(x=>x.id)).toEqual(["a","b"]);expect(b.next).toBeNull();
 });
 it("rejects repeated cursor or duplicate source",()=>{
  expect(()=>mergeSourcePage([source("a")],{items:[source("a")],next_after_id:null},"a",new Set())).toThrow();
  expect(()=>mergeSourcePage([],{items:[],next_after_id:"a"},"b",new Set(["a"]))).toThrow();
 });
});
