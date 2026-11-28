import {it,expect} from "vitest";
import {emptyLayerView,cleanLayerView} from "./LayerProperties";
import {copyLabelStyle,patchLayerViews,soloVisibility,batchLabelVisibility} from "./layer-actions";
import {tileLayerPolicy} from "./tile-layer-policy";
import {labelRulesError,labelVariants} from "./label-rules";
import {changeLabelOverride,overrideFor} from "./label-overrides";
import {planGpuLabels,labelLayer,viewportLabelPlan,uniqueVisibleLabels} from "./gpu-labels";
import type {StudioRow} from "./studio-contract";
import type {Geometry} from "geojson";
const row=(geometry:Geometry):StudioRow=>({key:"f1",layer:{id:"l",source_id:"s",source:{schema_revision:"schema",content_revision:"rev"}},geometry} as StudioRow);
const v={...emptyLayerView,labelField:"name",labelMinZoom:0};
it("keeps locks, selection and content independent while copying label style",()=>{
 const src={...v,locked:true,selectable:false,labelColor:"#123456",labelField:"name"},dst={...v,labelField:"other"};
 const copy=copyLabelStyle(src),next=patchLayerViews({a:dst},["a"],v,copy);
 expect(next.a.labelColor).toBe("#123456");expect(next.a.labelField).toBe("other");expect(next.a.locked).toBe(false);expect(next.a.selectable).toBe(true);
 expect(batchLabelVisibility({...v,labelField:""},false)).toEqual({});
});
it("batches 100 layer view changes without mutating inputs",()=>{
 const ids=Array.from({length:100},(_,i)=>"l"+i),views=patchLayerViews({},ids,v,{locked:true,selectable:false});
 expect(Object.keys(views)).toHaveLength(100);expect(Object.values(views).every(x=>x.locked&&!x.selectable)).toBe(true);expect(v.locked).toBe(false);
});
it("solo visibility restores previous hidden states",()=>{
 const a=soloVisibility(["a","b","c"],"a",null,new Set(["b"]));expect(a.show).toEqual(["a"]);expect(a.hide).toEqual(["b","c"]);
 const b=soloVisibility(["a","b","c"],"a",a.restore,new Set(["b","c"]));expect(b.show).toEqual(["a","c"]);expect(b.hide).toEqual(["b"]);expect(b.restore).toBeNull();
});
it("tile policy detects missing attribute support but not empty overrides",()=>{
 expect(tileLayerPolicy(cleanLayerView({})).unsupported).toBe(false);
 expect(tileLayerPolicy({...v,filterField:"name"}).unsupported).toBe(true);
 expect(tileLayerPolicy({...v,renderer:"categorized"}).unsupported).toBe(true);
 expect(tileLayerPolicy({...v,selectable:false,minZoom:10,opacity:.4})).toMatchObject({selectable:false,min:10,opacity:.4,unsupported:false});
});
it("validates nonoverlapping scale rules and fields",()=>{
 const a={min:10,max:15,field:"name",expression:"",size:12,color:"#123456"};
 const b={...a,min:15,max:22,field:"num",size:20};
 expect(labelRulesError([a,b],["name","num"])).toBeNull();
 expect(labelRulesError([a,{...b,min:14}],["name","num"])).not.toBeNull();
 expect(labelRulesError([a,b],["name"])).not.toBeNull();
 const views=labelVariants({...v,labelRules:[a,b]});expect(views.map(x=>[x.view.labelMinZoom,x.view.labelMaxZoom,x.view.labelField])).toEqual([[10,15,"name"],[15,22,"num"]]);
 const plan=planGpuLabels([row({type:"Point",coordinates:[1,1]})],{l:{...v,labelRules:[a,b],labelUnique:true}},(_,view)=>view.labelField,()=>null);
 expect(plan.groups.map(g=>labelLayer(g).minzoom)).toEqual([10,15]);
 expect([...uniqueVisibleLabels(plan,[0,0,2,2],16).values()].flat()).toHaveLength(1);
});
it("label override pin changes presentation only and is revision bound",()=>{
 const r=row({type:"Polygon",coordinates:[[[0,0],[10,0],[10,10],[0,10],[0,0]]]});
 const next=changeLabelOverride(r,{}, {position:[2,3],text:"manual",rotation:30});
 const p=planGpuLabels([r],{l:v},()=>"auto",()=>null,next);
 expect(p.data.features[0].geometry).toEqual({type:"Point",coordinates:[2,3]});expect(p.data.features[0].properties.text).toBe("manual");expect(p.polygons.size).toBe(0);expect(r.geometry?.type).toBe("Polygon");
 const stale={...r,layer:{...r.layer,source:{...r.layer.source,content_revision:"new"}}};expect(overrideFor(stale,next)).toBeUndefined();
 expect(planGpuLabels([r],{l:v},()=>"auto",()=>null,changeLabelOverride(r,next,{hidden:true})).data.features).toHaveLength(0);
});

import {mixedState,layerLockState} from "./layer-actions";
it("uses deterministic mixed states and distinguishes readonly from editable locks",()=>{
 expect(mixedState([])).toBe(false);expect(mixedState([true,false])).toBe("mixed");expect(mixedState([true,true])).toBe(true);
 expect(layerLockState(false,false,false)).toBe("readonly");expect(layerLockState(true,false,true)).toBe("editing");expect(layerLockState(true,true,false)).toBe("locked");expect(layerLockState(true,false,false)).toBe("unlocked");
});
