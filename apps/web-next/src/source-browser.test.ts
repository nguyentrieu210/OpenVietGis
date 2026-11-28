import {describe,it,expect} from "vitest";
import {mergeSourcePage,SOURCE_BROWSER_PAGE_LIMIT,SOURCE_BROWSER_SCAN_LIMIT} from "./source-browser";
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

it("tracks duplicate retired identities across pages, not just visible rows",()=>{
 const a=mergeSourcePage([],{items:[source("r","retired")],next_after_id:"cursor1"},null,new Set());
 expect(a.items).toEqual([]);expect(a.scanned.has("r")).toBe(true);
 expect(()=>mergeSourcePage(a.items,{items:[source("r","retired")],next_after_id:"cursor2"},"cursor1",new Set(["cursor1"]),a.scanned)).toThrow();
});
it("bounds retired-only scans and rejects empty advancing pages",()=>{
 const seen=new Set(Array.from({length:SOURCE_BROWSER_PAGE_LIMIT-1},(_,i)=>"c"+i));
 const r=mergeSourcePage([],{items:[source("r","retired")],next_after_id:"next"},null,seen);
 expect(r.limit).toBe(true);expect(r.items).toEqual([]);
 expect(()=>mergeSourcePage([],{items:[],next_after_id:"next"},null,new Set())).toThrow();
 const ids=new Set(Array.from({length:SOURCE_BROWSER_SCAN_LIMIT},(_,i)=>"r"+i));
 expect(()=>mergeSourcePage([],{items:[source("new")],next_after_id:null},null,new Set(),ids)).toThrow();
});
