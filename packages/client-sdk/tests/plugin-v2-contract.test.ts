import {describe,it,expect} from "vitest";
import {parseReadOnlyPluginDocument} from "../src/plugin-v2-contract";
import context from "../../plugin-sdk/examples/v2/host-context.json";
import notice from "../../plugin-sdk/examples/v2/host-notice.json";
import layers from "../../plugin-sdk/examples/v2/host-layers.json";
describe("shared universal declarative contract",()=>{
 const cases=[["context","host_context",context],["notice","host_notice",notice],["layers","map_view",layers]] as const;
 it("accepts all three Python-signed reference documents",()=>{
  for(const [name,cap,doc] of cases){
   const key="partner_fixture_"+name;
   expect(parseReadOnlyPluginDocument(doc,["ext."+key+"."+cap],key)).toEqual(doc);
  }
 });
 it("denies unknown widgets, commands, consent and fields",()=>{
  const key="partner_fixture_context",permissions=["ext."+key+".host_context"];
  for(const patch of [
   {...context,widgets:[{kind:"script",id:"x",text:"bad"}]},
   {...context,widgets:[{kind:"button",id:"x",label:"Run",action:"missing"}]},
   {...context,actions:[{...context.actions[0],command_id:"gis_layers"}]},
   {...context,actions:[{...context.actions[0],consent:"mutation"}]},
   {...context,actions:[{...context.actions[0],input:{extra:{kind:"text",required:false}}}]},
  ]) expect(()=>parseReadOnlyPluginDocument(patch,permissions,key)).toThrow("INVALID_PLUGIN_V2_CONTRACT");
 });
});
