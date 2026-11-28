import {describe,it,expect} from "vitest";
import type {SpatialLayer} from "@openvietgis/client-sdk";
import {relationRules,relationOptions,type ProjectRelation} from "./relation-values";
const uid=(n:number)=>"12345678-1234-1234-1234-"+String(n).padStart(12,"0");
const rule:ProjectRelation={id:uid(1),organization_id:uid(2),project_id:uid(3),source_id:uid(4),target_source_id:uid(5),local_field:"ward",target_field:"key",cardinality:"one",field_type:"decimal"};
const target={source_id:rule.target_source_id,source:{schema:{fields:[{field_id:"key",name:"Key",type:"decimal",nullable:true}]}}} as SpatialLayer;
const row=(id:string,v:unknown)=>({feature_id:id,attributes:{key:v}});
describe("project relation value picker",()=>{
 it("pins tenant/project and source",()=>{
  expect(relationRules({items:[rule]},uid(2),uid(3),uid(4))).toEqual([rule]);
  expect(relationRules({items:[rule]},uid(2),uid(3),uid(8))).toEqual([]);
  expect(()=>relationRules({items:[rule]},uid(8),uid(3),uid(4))).toThrow();
  expect(()=>relationRules({items:[rule,rule]},uid(2),uid(3),uid(4))).toThrow();
 });
 it("preserves precise decimal keys and excludes nulls",()=>{
  expect(relationOptions(rule,target,[row("a","12345678901234567890.123456789"),row("b",null),row("c","0.00")])).toEqual([{value:"12345678901234567890.123456789",count:1,first:"a"},{value:"0",count:1,first:"c"}]);
  expect(()=>relationOptions(rule,target,[row("a","1.0"),row("b","1")])).toThrow();
  expect(relationOptions({...rule,cardinality:"many"},target,[row("a","1.0"),row("b","1")])).toEqual([{value:"1",count:2,first:"a"}]);
 });
 it("rejects mismatched source/type",()=>{
  expect(()=>relationOptions({...rule,target_source_id:uid(7)},target,[row("a","1")])).toThrow();
  expect(()=>relationOptions({...rule,field_type:"integer"},target,[row("a","1")])).toThrow();
 });
});
