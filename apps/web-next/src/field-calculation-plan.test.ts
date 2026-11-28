import {describe,it,expect} from "vitest";
import {planFieldCalculation} from "./field-calculation-plan";
const fields=[{field_id:"n",name:"N",type:"decimal",nullable:false},{field_id:"label",name:"Label",type:"text",nullable:true},{field_id:"flag",name:"Flag",type:"boolean",nullable:false}];
const rows=[{feature_id:"a",attributes:{n:"9007199254740993.01",label:"x",flag:false}},{feature_id:"b",attributes:{n:"2.01",label:null,flag:true}}];
describe("field calculator",()=>{
 it("uses exact decimal arithmetic",()=>{expect(planFieldCalculation(fields,"n",rows,"[n]+0.01","exact").map(r=>r.value)).toEqual(["9007199254740993.02","2.02"])});
 it("supports typed conditional expressions and null",()=>{expect(planFieldCalculation(fields,"label",rows,'if("flag", NULL, upper("label"))',"ast").map(r=>r.value)).toEqual(["X",null]);expect(()=>planFieldCalculation(fields,"flag",rows,"NULL","ast")).toThrow("a:")});
 it("rejects partial unsafe plans",()=>{expect(()=>planFieldCalculation(fields,"n",rows,"0.1+0.2","ast")).toThrow();expect(()=>planFieldCalculation(fields,"n",[rows[0]!,rows[0]!],"1","exact")).toThrow();expect(rows[0]!.attributes.label).toBe("x")});
});
