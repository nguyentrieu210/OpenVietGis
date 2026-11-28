import {describe,it,expect,vi} from "vitest";
import type {SpatialLayer,LayerQueryResponse} from "@openvietgis/client-sdk";
import {combineQuerySelection,collectQuerySelection,sameKeys,collectExpressionSelection} from "./query-selection";
const layer={id:"l",source_id:"s",source:{content_revision:"c",schema_revision:"v",feature_count:101,schema:{fields:[]}}} as unknown as SpatialLayer;
const page=(offset:number,ids:string[],fp="pin")=>({layer_id:"l",source_id:"s",content_revision:"c",schema_revision:"v",offset,limit:100,query_fingerprint:fp,attribute_only:true,total_count:101,has_more:offset===0,items:ids.map(feature_id=>({feature_id,attributes:{},geometry:null,geometry_state:"absent"}))} as LayerQueryResponse);
const first=page(0,Array.from({length:100},(_,i)=>"id"+i));
const filters=[{field:"",op:"eq" as const,value:"",negate:false}];
describe("query selection",()=>{
 it("applies scoped set algebra and preserves other sources",()=>{
  const a=new Set(["s:a","s:b","other:k"]),b=new Set(["s:b","s:c"]);
  expect(combineQuerySelection(a,"s",b,"replace")).toEqual(new Set(["other:k","s:b","s:c"]));
  expect(combineQuerySelection(a,"s",b,"add")).toEqual(new Set(["s:a","s:b","s:c","other:k"]));
  expect(combineQuerySelection(a,"s",b,"remove")).toEqual(new Set(["s:a","other:k"]));
  expect(combineQuerySelection(a,"s",b,"intersect")).toEqual(new Set(["s:b","other:k"]));
  expect(combineQuerySelection(a,"s",b,"invert")).toEqual(new Set(["s:c","other:k"]));
  expect(()=>combineQuerySelection(a,"s",new Set(["other:x"]),"replace")).toThrow();
  expect(sameKeys(a,new Set([...a].reverse()))).toBe(true);
 });
 it("collects all pages pinned to preview including absent geometries",async()=>{
  const request=vi.fn(async(b:Record<string,unknown>)=>b.offset===0?first:page(100,["last"]));
  const r=await collectQuerySelection(layer,first,filters,"","asc","and","replace",request,()=>true,()=>{});
  expect(r.keys.size).toBe(101);expect(r.missingGeometry).toBe(101);
  expect(request.mock.calls[0]?.[0]).toMatchObject({offset:0,expected_query_fingerprint:"pin"});expect(request.mock.calls[1]?.[0]).toMatchObject({offset:100,expected_query_fingerprint:"pin"});
 });
 it("rejects duplicate pages, changed fingerprint, selection change and oversized source",async()=>{
  const run=(request:(b:Record<string,unknown>)=>Promise<LayerQueryResponse>,current=()=>true)=>collectQuerySelection(layer,first,filters,"","asc","and","replace",request,current,()=>{});
  await expect(run(async b=>b.offset===0?first:page(100,["id0"]))).rejects.toThrow("Trùng FID");
  await expect(run(async b=>b.offset===0?first:page(100,["last"],"wrong"))).rejects.toThrow();
  let active=true;await expect(run(async()=>{active=false;return first},()=>active)).rejects.toThrow();
  await expect(collectQuerySelection({...layer,source:{...layer.source,feature_count:5001}},first,filters,"","asc","and","invert",async()=>first,()=>true,()=>{})).rejects.toThrow();
 });
});

describe("typed expression selection",()=>{
 it("checks full pinned population, excludes null, includes absent geometry",async()=>{
  const l={...layer,source:{...layer.source,feature_count:3}};
  const response={...page(0,["a","b","c"]),total_count:3,has_more:false} as LayerQueryResponse;
  const found=await collectExpressionSelection(l,"TRUE",async()=>response,()=>true,()=>{});
  expect(found.keys).toEqual(new Set(["s:a","s:b","s:c"]));expect(found.missingGeometry).toBe(3);
  expect((await collectExpressionSelection(l,"NULL",async()=>response,()=>true,()=>{})).keys.size).toBe(0);
  await expect(collectExpressionSelection(l,"42",async()=>response,()=>true,()=>{})).rejects.toThrow("Boolean");
  await expect(collectExpressionSelection(l,"TRUE",async()=>response,()=>false,()=>{})).rejects.toThrow();
 });
});
