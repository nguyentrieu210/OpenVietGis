// Generic discovery only; never name a business module in Core.
import type {ExtensionActivation,ExtensionManifest} from "@openvietgis/client-sdk";
import type {StudioPlugin} from "./studio-contract";
export type Manifest={extension_key:string;display_name:string;version:string;description?:string;api_compatibility:string;permissions?:string[]};
const manifests=Object.values(import.meta.glob<Manifest>("../../../modules/*/manifest.json",{eager:true,import:"default"}));
const loaders=import.meta.glob<StudioPlugin>("../../../modules/*/frontend/studio.tsx",{import:"default"});
export const installed=manifests.filter(m=>m.api_compatibility==="1.x"&&Object.keys(loaders).some(path=>path.includes("/modules/"+m.extension_key+"/")));
export const loadPlugin=(id:string)=>Object.entries(loaders).find(([path])=>path.includes("/modules/"+id+"/"))?.[1]();

export const compatibleActivation=(m:Manifest,a:ExtensionActivation)=>a.extension_key===m.extension_key&&a.status==="active"&&a.manifest_version===m.version&&a.runtime_compatible===true;
export const compatibleRegistration=(m:Manifest,a:ExtensionManifest)=>a.extension_key===m.extension_key&&a.version===m.version&&a.api_compatibility===m.api_compatibility&&a.enabled===true;
