import {describe,it,expect} from "vitest";
import type {SpatialLayer} from "@openvietgis/client-sdk";
import type {StudioRow} from "./studio-contract";
import type {FeatureCollection} from "geojson";
import {timestampMs,temporalPopulation,temporalCollection} from "./temporal-plan";
const layer={id:"l",source_id:"s",lifecycle:"active",source:{feature_count:3,content_revision:"c",schema_revision:"v",schema:{fields:[{field_id:"time",type:"timestamp"}]}},capabilities:[{name:"feature_read",available:true,actor_authorized:true,technical_supported:true}]} as unknown as SpatialLayer;
const row=(key:string,time:unknown)=>({key,layer,feature:{feature_id:key,attributes:{time}},geometry:null}) as unknown as StudioRow;
const rows=[row("a","2026-11-28T00:30:00+07:00"),row("b","2026-11-28T00:00:00Z"),row("c",null)];
describe("temporal vector frames",()=>{
 it("uses strict timezone-aware timestamps and rejects invalid calendar dates",()=>{
  expect(timestampMs("2026-11-28T00:30:00+07:00")).toBe(Date.parse("2026-11-27T17:30:00Z"));
  for(const x of ["2026-02-30T00:00:00Z","2026-11-28","2026-11-28T01:00:00","2026-11-28T24:00:00Z","2026-11-28T00:00:00+24:00","2026-11-28T00:00:00+00:60",null,123])expect(timestampMs(x)).toBeNull();
 });
 it("counts invalid timestamps, splits UTC days and fails closed on changed sources or missing rows",()=>{
  const p=temporalPopulation(layer,rows,"time");
  expect(p.days).toEqual([Date.parse("2026-11-27T00:00:00Z"),Date.parse("2026-11-28T00:00:00Z")]);
  expect(p.invalid).toBe(1);expect([...p.byDay.get(p.days[0]!)!]).toEqual(["l:a"]);
  expect(()=>temporalPopulation(layer,rows.slice(0,2),"time")).toThrow();
  expect(()=>temporalPopulation(layer,[rows[0]!,{...rows[1]!,key:rows[0]!.key},rows[2]!],"time")).toThrow();
  expect(()=>temporalPopulation(layer,[{...rows[0]!,key:""},rows[1]!,rows[2]!],"time")).toThrow();
  expect(()=>temporalPopulation(layer,[rows[0]!,{...rows[1]!,feature:{...rows[1]!.feature,feature_id:rows[0]!.feature.feature_id}},rows[2]!],"time")).toThrow();
  expect(()=>temporalPopulation({...layer,source:{...layer.source,content_revision:"changed"}},rows,"time")).toThrow();
  expect(()=>temporalPopulation({...layer,capabilities:[]} as SpatialLayer,rows,"time")).toThrow();
 });
 it("renders only matching authorized input features and clears editing flags",()=>{
  const f=(id:string)=>({type:"Feature" as const,id,geometry:{type:"Point" as const,coordinates:[106,11]},properties:{selected:true,selectable:true}});
  const input:FeatureCollection={type:"FeatureCollection",features:[f("l:a"),f("l:b"),f("foreign:a")]};
  const out=temporalCollection(input,new Set(["l:a"]));
  expect(out.features.map(x=>x.id)).toEqual(["l:a"]);expect(out.features[0]!.properties).toMatchObject({selected:false,selectable:false});expect(input.features[0]!.properties?.selected).toBe(true);
 });
});
