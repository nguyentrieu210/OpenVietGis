import {describe,it,expect} from "vitest";
import type {SpatialLayer} from "@openvietgis/client-sdk";
import type {StudioRow} from "./studio-contract";
import {relationMatches,readRelationSpec} from "./relation-inspector";
const layer=(id:string,n:number)=>({id,source_id:id,lifecycle:"active",capabilities:[{name:"feature_read",available:true,technical_supported:true,actor_authorized:true}],source:{content_revision:"c",schema_revision:"s",feature_count:n,schema:{fields:[{field_id:"k",type:"integer"}]}}} as unknown as SpatialLayer);
const a=layer("a",1),b=layer("b",2);
const row=(l:SpatialLayer,id:string,k:unknown)=>({key:l.id+":"+id,layer:l,feature:{feature_id:id,attributes:{k}},geometry:null} as unknown as StudioRow);
const src=row(a,"src","9007199254740993"),r1=row(b,"r1","9007199254740993"),r2=row(b,"r2","9007199254740992");
const spec={target:"b",localField:"k",targetField:"k",cardinality:"one" as const};
describe("relation inspector",()=>{
 it("matches exact integer keys beyond JS precision, not unrelated rows",()=>{expect(relationMatches(src,[a,b],[r1,r2],spec,false)).toEqual([r1]);expect(relationMatches(row(a,"src",null),[a,b],[r1,r2],spec,false)).toEqual([])});
 it("rejects partial populations, revoked access, duplicate cardinality and restricted scope",()=>{
  expect(()=>relationMatches(src,[a,b],[r1],spec,false)).toThrow();
  expect(()=>relationMatches(src,[a,{...b,capabilities:[]}],[r1,r2],spec,false)).toThrow();
  expect(()=>relationMatches(src,[a,b],[r1,row(b,"r2","9007199254740993")],spec,false)).toThrow();
  expect(()=>relationMatches(src,[a,b],[r1,r2],spec,true)).toThrow();
  expect(readRelationSpec({...spec,cardinality:"invalid"})).toBeNull();
 });
});
