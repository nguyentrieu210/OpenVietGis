import type {ExtensionManifest,ExtensionActivation} from "@openvietgis/client-sdk";
import {partnerPanel} from "@openvietgis/client-sdk";
import {partnerTool,type PartnerTool,type PartnerMapMethod} from "./partner-map-bridge";
import type {CoreCommand} from "./CoreCommandBar";
export function activePartner(m:ExtensionManifest,a:readonly ExtensionActivation[],method?:PartnerMapMethod){
 const panel=partnerPanel(m),matches=a.filter(x=>x.extension_key===m.extension_key);
 return !!panel&&matches.length===1&&matches[0]?.manifest_id===m.id&&matches[0]?.manifest_version===m.version&&matches[0]?.status==="active"&&(!method||panel.capabilities.includes(method));
}
export function partnerActivationPin(m:ExtensionManifest,a:readonly ExtensionActivation[]){
 if(!activePartner(m,a))return "";
 const activation=a.find(x=>x.extension_key===m.extension_key)!;
 return JSON.stringify([m.id,m.version,activation.id,activation.revision??activation.updated_at,partnerPanel(m)?.url,partnerPanel(m)?.capabilities]);
}
export function partnerCommands(tools:readonly PartnerTool[],existing:readonly {alias:string}[],disabled:boolean):CoreCommand[]{
 const used=new Set(existing.map(c=>c.alias.toUpperCase())),out:CoreCommand[]=[];
 for(const t of tools.slice(0,4)){
  const safe=partnerTool(t);if(!safe||typeof t.run!=="function")continue;
  const alias="PARTNER_"+safe.id.toUpperCase();if(used.has(alias))continue;used.add(alias);
  out.push({alias,label:"Đối tác · "+safe.label,group:"Plugin",disabled,run:()=>{if(!disabled)return t.run()}});
 }
 return out;
}
