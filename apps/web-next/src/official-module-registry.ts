// Generic discovery only; never name a business module in Core.
import type {StudioPlugin} from "./studio-contract";
export type Manifest={extension_key:string;display_name:string;version:string;description?:string};
const manifests=Object.values(import.meta.glob<Manifest>("../../../modules/*/manifest.json",{eager:true,import:"default"}));
const loaders=import.meta.glob<StudioPlugin>("../../../modules/*/frontend/studio.tsx",{import:"default"});
export const installed=manifests.filter(m=>Object.keys(loaders).some(path=>path.includes("/modules/"+m.extension_key+"/")));
export const loadPlugin=(id:string)=>Object.entries(loaders).find(([path])=>path.includes("/modules/"+id+"/"))?.[1]();
