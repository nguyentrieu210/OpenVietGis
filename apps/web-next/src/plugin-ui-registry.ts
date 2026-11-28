import type {CoreCommand} from "./CoreCommandBar";
export type PluginAction={id:string;label:string;run:()=>void;disabled?:boolean;toolbar?:boolean;shortLabel?:string};
export function pluginUICommands(id:string,actions:readonly PluginAction[],existing:readonly {alias:string}[],blocked:boolean):CoreCommand[]{
 if(!/^[a-z][a-z0-9_]{0,63}$/.test(id))return [];
 const used=new Set(existing.map(x=>x.alias.toUpperCase())),out:CoreCommand[]=[];
 for(const a of actions.slice(0,40)){if(!/^[a-z][a-z0-9_]{0,47}$/.test(a.id)||typeof a.run!=="function"||!a.label.trim())continue;
 const alias=("PLUGIN_"+id+"_"+a.id).toUpperCase();if(used.has(alias))continue;used.add(alias);
 const disabled=blocked||!!a.disabled;out.push({alias,label:a.label.slice(0,120),group:"Plugin",disabled,run:()=>{if(!disabled)a.run()}})
 }return out;
}
