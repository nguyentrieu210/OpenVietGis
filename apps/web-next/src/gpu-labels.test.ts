import {describe,expect,it} from "vitest";
import type {Geometry} from "geojson";
import type {LayerView} from "./LayerProperties";
import type {StudioRow} from "./studio-contract";
import {planGpuLabels,labelLayer,uniqueVisibleLabels} from "./gpu-labels";
const view={labelField:"name",labelHidden:false,labelMinZoom:0,labelMaxZoom:22,minZoom:0,maxZoom:22,labelPriority:5,labelPlacement:"auto",labelSize:14,labelOffset:7,labelColor:"#123456",labelHalo:"#ffffff",labelHaloWidth:1,labelBold:true,labelBackground:false,labelOverlap:false,labelUnique:false} as LayerView;
const row=(key:string,geometry:Geometry,layer="l"):StudioRow=>({key,geometry,layer:{id:layer}} as StudioRow);
const point=(x:number,y:number):Geometry=>({type:"Point",coordinates:[x,y]});
const label=()=> "Đường số 1";
const anchor=()=>[0,0] as [number,number];
describe("GPU label planning",()=>{
 it("anchors polygons as single points, retains line geometry and emits native symbols",()=>{
  const plan=planGpuLabels([row("p",point(0,0)),row("poly",{type:"Polygon",coordinates:[[[0,0],[1,0],[1,1],[0,0]]]}),row("line",{type:"LineString",coordinates:[[0,0],[1,1]]})],{l:view},label,anchor);
  expect(plan.data.features.map(f=>f.geometry.type)).toEqual(["Point","Point","LineString"]);
  expect(plan.groups.map(g=>labelLayer(g).layout?.["symbol-placement"]).sort()).toEqual(["line-center","point","point"]);
  expect(plan.data.features[0]?.properties.text).toBe("Đường số 1");
 });
 it("honors explicit positioning, color, scale and background",()=>{
  const v={...view,labelPlacement:"above" as const,labelBackground:true,labelOverlap:true,labelMinZoom:9,maxZoom:17};
  const plan=planGpuLabels([row("a",point(0,0))],{l:v},label,anchor),s=labelLayer(plan.groups[0]!);
  expect(s.minzoom).toBe(9);expect(s.maxzoom).toBe(17);
  expect(s.layout?.["text-anchor"]).toBe("bottom");
  expect(s.layout?.["text-radial-offset"]).toBe(.5);
  expect(s.layout?.["text-allow-overlap"]).toBe(true);
  expect(s.layout?.["icon-text-fit"]).toBe("both");
  expect(s.paint?.["text-color"]).toBe("#123456");
 });
 it("chooses a visible representative per duplicate text without changing other layers",()=>{
  const plan=planGpuLabels([row("a",point(0,0)),row("b",point(10,10)),row("c",point(10.1,10.1)),row("d",point(10,10),"x")],{l:{...view,labelUnique:true},x:view},label,anchor);
  expect(uniqueVisibleLabels(plan,[9,9,11,11]).get("l:point")).toEqual(["l:b"]);
  expect(uniqueVisibleLabels(plan,[-1,-1,1,1]).get("l:point")).toEqual(["l:a"]);
  expect(uniqueVisibleLabels(plan,[9,9,11,11]).has("x:point")).toBe(false);
 });
 it("does not add labels from hidden layers or invalid scale ranges",()=>{
  expect(planGpuLabels([row("a",point(0,0))],{l:{...view,labelHidden:true}},label,anchor).groups).toEqual([]);
  expect(planGpuLabels([row("a",point(0,0))],{l:{...view,labelMinZoom:15,maxZoom:14}},label,anchor).data.features).toEqual([]);
 });
});

import {polygonLabelAnchor} from "./polygon-label-anchor";
it("keeps one interior anchor outside holes and deduplicates repeated source rows",()=>{
 const g:Geometry={type:"Polygon",coordinates:[[[0,0],[10,0],[10,10],[0,10],[0,0]],[[3,3],[7,3],[7,7],[3,7],[3,3]]]};
 const p=polygonLabelAnchor(g)!;expect(p[0]>0&&p[0]<10&&p[1]>0&&p[1]<10).toBe(true);expect(p[0]>3&&p[0]<7&&p[1]>3&&p[1]<7).toBe(false);
 const r=row("same",g);const plan=planGpuLabels([r,r],{l:view},label,anchor);expect(plan.data.features).toHaveLength(1);expect(plan.data.features[0].geometry).toEqual({type:"Point",coordinates:p});
});
it("uses one interior point for a multipart feature",()=>{
 const g:Geometry={type:"MultiPolygon",coordinates:[[[[0,0],[1,0],[1,1],[0,1],[0,0]]],[[[10,10],[20,10],[20,20],[10,20],[10,10]]]]};
 const plan=planGpuLabels([row("multi",g)],{l:view},label,anchor);expect(plan.data.features).toHaveLength(1);expect(plan.data.features[0].geometry).toEqual({type:"Point",coordinates:[15,15]});
});
it("keeps anchor within a concave polygon",()=>{
 const g:Geometry={type:"Polygon",coordinates:[[[0,0],[8,0],[8,2],[2,2],[2,8],[0,8],[0,0]]]};
 const p=polygonLabelAnchor(g)!;expect(p[0]>0&&p[1]>0&&p[0]<8&&p[1]<8&&(p[0]<2||p[1]<2)).toBe(true);
});
