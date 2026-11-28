import {describe,it,expect} from "vitest";
import {cleanFormLayout,cleanValueMap,valueMapSelection,formGroups,formValues,moveFormField,validateForm,type FormField} from "./attribute-form-layout";
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

it("canonicalizes personal value maps without constraining canonical writes",()=>{
 const field=fields[1]!;
 const choices=cleanValueMap([{value:"1.00",label:"One"},{value:"1",label:"duplicate"},{value:"bad",label:"invalid"},{value:"12345678901234567890.123456789",label:"Exact"}],field);
 expect(choices).toEqual([{value:"1",label:"One"},{value:"12345678901234567890.123456789",label:"Exact"}]);
 expect(valueMapSelection(field,"1.000",choices)).toBe(0);
 expect(valueMapSelection(field,"2",choices)).toBe(-1);
 expect(cleanValueMap([{value:"true",label:"OK"}],fields[2]!)).toEqual([]);
 const layout=cleanFormLayout({version:1,fields:[{id:"n",widget:"value-map",choices}]},fields);
 expect(layout.fields[0]?.choices).toEqual(choices);
 expect(validateForm(fields,formValues(fields,{name:"other",n:"2",ok:true})).properties.n).toBe("2");
});
