import type {StudioRow} from "./studio-contract";
export type LabelOverride={source:string;schema:string;revision:string;position?:[number,number];rotation?:number;text?:string;hidden?:boolean;callout?:boolean};
export type LabelOverrides=Record<string,LabelOverride>;
export const overrideKey=(row:StudioRow)=>JSON.stringify([row.layer.id,row.key]);
export function cleanLabelOverrides(raw:unknown):LabelOverrides{
 const out:LabelOverrides=Object.create(null);
 if(!raw||typeof raw!=="object")return out;
 for(const [k,value] of Object.entries(raw).slice(0,500)){
  if(!k.startsWith("[")||k.length>600||!value||typeof value!=="object")continue;
  const v=value as Partial<LabelOverride>;if(typeof v.source!=="string"||typeof v.schema!=="string"||typeof v.revision!=="string")continue;
  const p=v.position;const position=Array.isArray(p)&&p.length===2&&p.every(Number.isFinite)&&Math.abs(p[0])<=180&&Math.abs(p[1])<=90?[p[0],p[1]] as [number,number]:undefined;
  out[k]={source:v.source,schema:v.schema,revision:v.revision,position,rotation:Number.isFinite(v.rotation)?Math.max(-180,Math.min(180,v.rotation!)):undefined,text:typeof v.text==="string"?v.text.slice(0,150):undefined,hidden:v.hidden===true,callout:v.callout===true};
 }
 return out;
}
export function overrideFor(row:StudioRow,overrides:LabelOverrides|undefined){
 const v=overrides?.[overrideKey(row)],s=row.layer.source;
 return v&&v.source===row.layer.source_id&&v.schema===s.schema_revision&&v.revision===s.content_revision?v:undefined;
}
export function changeLabelOverride(row:StudioRow,overrides:LabelOverrides,patch:Partial<LabelOverride>|null){
 const k=overrideKey(row),next={...overrides};
 if(patch===null)delete next[k];else next[k]={...overrideFor(row,overrides),...patch,source:row.layer.source_id,schema:row.layer.source.schema_revision,revision:row.layer.source.content_revision};
 return cleanLabelOverrides(next);
}
