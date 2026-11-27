import {describe,it,expect} from "vitest";
import type {SpatialLayer,LayerQueryResponse} from "@openvietgis/client-sdk";
import {tableQueryBody,verifyTableQuery} from "./table-query";
const layer={id:"layer",source_id:"source",source:{content_revision:"c",schema_revision:"s",schema:{fields:[{field_id:"name",name:"Name",type:"text",nullable:true}]}}} as unknown as SpatialLayer;
const filter={field:"name",op:"contains" as const,value:"%_';",negate:false};
const result={layer_id:"layer",source_id:"source",content_revision:"c",schema_revision:"s",offset:100,limit:100,query_fingerprint:"pin"} as LayerQueryResponse;
describe("table server query",()=>{
 it("uses typed literal filter, pinned revision and server sort",()=>{
  expect(tableQueryBody(layer,filter,"name","desc",100,"pin")).toEqual({attribute_filter:{field:"name",op:"contains",value:"%_';"},sort_field:"name",sort_direction:"desc",expected_content_revision:"c",expected_schema_revision:"s",limit:100,offset:100,expected_query_fingerprint:"pin"});
 });
 it("rejects unpinned or oversized pagination and unknown sort",()=>{
  expect(()=>tableQueryBody(layer,filter,"name","asc",100)).toThrow();
  expect(()=>tableQueryBody(layer,filter,"name","asc",25100,"pin")).toThrow();
  expect(()=>tableQueryBody(layer,filter,"unknown","asc",0)).toThrow();
 });
 it("rejects stale or foreign results before rendering",()=>{
  expect(verifyTableQuery(layer,result,100,"pin")).toBe(result);
  for(const patch of [{source_id:"foreign"},{schema_revision:"other"},{query_fingerprint:"wrong"},{offset:0},{limit:50}])expect(()=>verifyTableQuery(layer,{...result,...patch},100,"pin")).toThrow();
 });
});
describe("multiple server filters",()=>{
 it("preserves AND/OR and NOT with literal values",()=>{
  const second={...filter,value:"a",negate:true};
  expect(tableQueryBody(layer,[filter,second],"","asc",0,undefined,"or").attribute_filter).toEqual({op:"or",conditions:[{field:"name",op:"contains",value:"%_';"},{op:"not",condition:{field:"name",op:"contains",value:"a"}}]});
 });
});
