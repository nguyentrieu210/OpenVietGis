import {describe,it,expect} from "vitest";
import {cleanFormLayout,formGroups,formValues,moveFormField,validateForm,type FormField} from "./attribute-form-layout";
const fields=[{field_id:"name",name:"Name",type:"text",nullable:false},{field_id:"n",name:"Number",type:"decimal",nullable:true},{field_id:"ok",name:"OK",type:"boolean",nullable:false}] as FormField[];
describe("personal attribute forms",()=>{
 it("keeps all current schema fields, removes stale and duplicate fields, respects widget type",()=>{
  const l=cleanFormLayout({version:1,fields:[{id:"n",group:"B",widget:"multiline"},{id:"n"},{id:"removed"},{id:"name",group:"A",widget:"multiline"}]},fields);
  expect(l.fields.map(f=>[f.id,f.widget])).toEqual([["n","auto"],["name","multiline"],["ok","auto"]]);
  expect(formGroups(l,fields).map(g=>g.name)).toEqual(["B","A",""]);
  expect(moveFormField(l,1,-1).fields.map(f=>f.id)).toEqual(["name","n","ok"]);
 });
 it("preserves null and exact decimals, rejects invalid fields before any write",()=>{
  const values=formValues(fields,{name:"plot",n:"12345678901234567890.123456789",ok:false});
  expect(validateForm(fields,values)).toEqual({properties:{name:"plot",n:"12345678901234567890.123456789",ok:false},errors:{}});
  expect(validateForm(fields,{...values,n:{raw:"",nil:true}}).properties.n).toBeNull();
  expect(validateForm(fields,{...values,ok:{raw:"",nil:false}}).errors.ok).toBeTruthy();
  expect(validateForm(fields,{...values,name:{raw:"",nil:true}}).errors.name).toBeTruthy();
 });
});
