import {describe,it,expect} from "vitest";
import {cleanDiagram,diagramValues,featureAction} from "./feature-diagram";
describe("native chart and allowlisted feature actions",()=>{
 it("bounds config and preserves signed numbers without coercing null or text",()=>{const d=cleanDiagram({fields:["a","b","c","d","a"],size:1000,minZoom:-5});expect(d).toEqual({fields:["a","b","c","d"],size:96,minZoom:0});const fields=[{field_id:"a",name:"A",type:"decimal"},{field_id:"b",name:"B",type:"integer"},{field_id:"c",name:"C",type:"decimal"},{field_id:"d",name:"D",type:"text"}];expect(diagramValues({a:"-5",b:10,c:null,d:"123"},fields,d).map(v=>v.fraction)).toEqual([-.5,1])});
 it("copies literal text and JSON without executing values",()=>{expect(featureAction("id","fid",{})).toBe("fid");expect(JSON.parse(featureAction("attributes","fid",{x:"<script>alert(1)</script>"}))).toEqual({x:"<script>alert(1)</script>"})});
});
