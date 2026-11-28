import {describe,it,expect} from "vitest";
import type {SpatialLayer,LayerQueryResponse} from "@openvietgis/client-sdk";
import {localTablePage,tableQueryBody,verifyTableQuery} from "./table-query";
const layer={id:"layer",source_id:"source",source:{content_revision:"c",schema_revision:"s",schema:{fields:[{field_id:"name",name:"Name",type:"text",nullable:true}]}}} as unknown as SpatialLayer;
const filter={field:"name",op:"contains" as const,value:"%_';",negate:false};
const result={layer_id:"layer",source_id:"source",content_revision:"c",schema_revision:"s",offset:100,limit:100,query_fingerprint:"pin",attribute_only:true,stored_crs:4326,selection_feature_ids:["a"],total_count:101,has_more:false,items:[{feature_id:"a",geometry:null,geometry_state:"absent",attributes:{name:"a"}}]} as LayerQueryResponse;
describe("table server query",()=>{
 it("uses typed literal filter, pinned revision and server sort",()=>{
  expect(tableQueryBody(layer,filter,"name","desc",100,"pin")).toEqual({attribute_only:true,attribute_filter:{field:"name",op:"contains",value:"%_';"},sort_field:"name",sort_direction:"desc",expected_content_revision:"c",expected_schema_revision:"s",limit:100,offset:100,expected_query_fingerprint:"pin"});
 });
 it("rejects unpinned or oversized pagination and unknown sort",()=>{
  expect(()=>tableQueryBody(layer,filter,"name","asc",100)).toThrow();
  expect(()=>tableQueryBody(layer,filter,"name","asc",50100,"pin")).toThrow();
  expect(()=>tableQueryBody(layer,filter,"unknown","asc",0)).toThrow();
 });
 it("rejects stale or foreign results before rendering",()=>{
  expect(verifyTableQuery(layer,result,100,"pin")).toBe(result);
  for(const patch of [{source_id:"foreign"},{schema_revision:"other"},{query_fingerprint:"wrong"},{offset:0},{limit:50},{attribute_only:undefined},{items:[]},{has_more:true}])expect(()=>verifyTableQuery(layer,{...result,...patch},100,"pin")).toThrow();
 });
});
describe("multiple server filters",()=>{
 it("preserves AND/OR and NOT with literal values",()=>{
  const second={...filter,value:"a",negate:true};
  expect(tableQueryBody(layer,[filter,second],"","asc",0,undefined,"or").attribute_filter).toEqual({op:"or",conditions:[{field:"name",op:"contains",value:"%_';"},{op:"not",condition:{field:"name",op:"contains",value:"a"}}]});
 });
});

it("accepts final page beyond old 25000 offset",()=>{expect(tableQueryBody(layer,filter,"name","asc",49900,"pin").offset).toBe(49900)});

it("rejects malformed attribute row identity and payload before use",()=>{
 const row=result.items[0]!;
 for(const patch of [{feature_id:""},{feature_id:123},{feature_id:"a".repeat(513)},{attributes:null},{attributes:[]},{attributes:"bad"}]){
  expect(()=>verifyTableQuery(layer,{...result,items:[{...row,...patch}]} as unknown as LayerQueryResponse,100,"pin")).toThrow();
 }
 expect(()=>verifyTableQuery(layer,{...result,query_fingerprint:"x".repeat(513)},100)).toThrow();
 expect(()=>tableQueryBody(layer,filter,"name","asc",100,"x".repeat(513))).toThrow();
describe("local attribute table pagination",()=>{
 it("bounds DOM rows to 100 even for one million records",()=>{
  const rows=Array.from({length:1_000_000},(_,i)=>i);
  expect(localTablePage(rows,0)).toEqual(Array.from({length:100},(_,i)=>i));
  expect(localTablePage(rows,9_999)).toHaveLength(100);
  expect(localTablePage(rows,10_000)).toHaveLength(0);
  expect(()=>localTablePage(rows,-1)).toThrow();
 });
});
