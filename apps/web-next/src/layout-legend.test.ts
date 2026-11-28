import {describe,it,expect} from "vitest";
import type {SpatialLayer} from "@openvietgis/client-sdk";
import type {StudioRow} from "./studio-contract";
import {emptyLayerView} from "./LayerProperties";
import {layoutLegend} from "./layout-legend";
const l={id:"a",name:"Zones",lifecycle:"active",source:{feature_count:3,schema:{geometry_family:"Polygon"}},capabilities:[{name:"feature_read",available:true,actor_authorized:true}]} as unknown as SpatialLayer;
const rows=["A","B"].map((zone,i)=>({layer:l,key:String(i),feature:{attributes:{zone,height:i+1}}})) as unknown as StudioRow[];
describe("layout legend",()=>{
 it("uses categorized symbols, marks partial input and omits hidden classes",()=>{
  const v={...emptyLayerView,renderer:"categorized" as const,categoryField:"zone",hiddenCats:["B"],categoryColors:{A:"#123456"}};
  const r=layoutLegend([l],rows,{a:v},()=>"#000000",12);
  expect(r[0]?.text).toContain("mẫu đã tải");expect(r[1]).toMatchObject({text:"A",color:"#123456",family:"Polygon"});expect(r).toHaveLength(2);
  expect(layoutLegend([l],rows,{a:{...v,minZoom:15}},()=>"#000000",12)).toEqual([]);
  expect(layoutLegend([{...l,capabilities:[]}],rows,{},()=>"#000000")).toEqual([]);
 });
 it("graduated legend preserves custom color",()=>{
  const v={...emptyLayerView,renderer:"graduated" as const,gradField:"height",gradBreaks:[1,1.5,2],gradColors:{"1":"#123456"}};
  expect(layoutLegend([l],rows,{a:v},()=>"#000000")[2]).toMatchObject({color:"#123456"});
 });
});
