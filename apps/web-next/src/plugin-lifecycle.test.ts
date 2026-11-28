import {describe,it,expect,vi} from "vitest";
import type {ExtensionManifest,ExtensionActivation} from "@openvietgis/client-sdk";
import {activePartner,partnerActivationPin,partnerCommands} from "./plugin-lifecycle";
const m={id:"m",extension_key:"partner_demo",version:"1.0.0",enabled:true,api_compatibility:"1.x",permissions:[],commands:[],entity_schemas:{},capabilities:["tool_register"],ui_contributions:[{id:"partner_panel",slot:"project_panel",route_suffix:"/partner",runtime:"sandboxed_iframe",sdk_version:"1.0",label:"Demo",url:"https://plugins.example.org/v1/index.html"}]} as unknown as ExtensionManifest;
const a={id:"a",manifest_id:"m",extension_key:m.extension_key,manifest_version:"1.0.0",status:"active",revision:1} as ExtensionActivation;
describe("partner lifecycle",()=>{
 it("requires exact active version and rejects duplicate activations",()=>{
  expect(activePartner(m,[a],"tool_register")).toBe(true);expect(activePartner(m,[a],"map_view")).toBe(false);
  expect(activePartner(m,[{...a,manifest_version:"2.0.0"}])).toBe(false);expect(activePartner(m,[a,a])).toBe(false);
  expect(partnerActivationPin(m,[a])).not.toBe(partnerActivationPin(m,[{...a,revision:2}]));
 });
 it("uses namespaced commands, rejects collisions and disables execution",()=>{
  const run=vi.fn(),t={id:"demo",label:"Demo",run};
  expect(partnerCommands([t],[{alias:"PARTNER_DEMO"}],false)).toEqual([]);
  const commands=partnerCommands([t,t],[],true);expect(commands.length).toBe(1);commands[0]!.run("");expect(run).not.toHaveBeenCalled();
  partnerCommands([t],[],false)[0]!.run("");expect(run).toHaveBeenCalledTimes(1);
 });
});
