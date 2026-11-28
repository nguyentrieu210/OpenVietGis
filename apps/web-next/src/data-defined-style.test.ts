import {describe,it,expect} from "vitest";
import {dataDefinedPaint,cleanDataDefined,hasDataDefined} from "./data-defined-style";
const fields=[{field_id:"c",type:"text"},{field_id:"o",type:"decimal"},{field_id:"w",type:"integer"},{field_id:"r",type:"decimal"}];
const config={color:"c",opacity:"o",width:"w",radius:"r"};
describe("typed paint overrides",()=>{
 it("maps only exact schema field values",()=>expect(dataDefinedPaint(config,{c:"#123456",o:"0.5",w:3,r:"6"},fields,.8)).toEqual({color:"#123456",opacity:.4,width:3,strokeWidth:3,radius:6}));
 it("rejects missing, invalid and out-of-range values",()=>expect(dataDefinedPaint(config,{c:"javascript:bad",o:null,w:500,r:""},fields)).toEqual({}));
 it("cleans persisted values and detects active overrides",()=>{expect(hasDataDefined({width:"w"})).toBe(true);expect(hasDataDefined(cleanDataDefined({width:1}))).toBe(false)});
});
