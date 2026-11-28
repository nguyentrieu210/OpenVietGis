/** Bounded, local-only per-feature bar diagrams. Null and invalid numbers are
 * omitted; no script, HTML, network access, aggregation or data mutation. */
export type DiagramField={field_id:string;name:string;type:string};
export type DiagramSettings={fields:string[];size:number;minZoom:number};
export const diagramColors=["#2563eb","#d97706","#059669","#9333ea","#dc2626","#0891b2"];
export function cleanDiagram(raw:unknown):DiagramSettings{
 const v=(raw&&typeof raw==="object"?raw:{}) as Partial<DiagramSettings>;
 return {fields:Array.isArray(v.fields)?[...new Set(v.fields.filter(x=>typeof x==="string"&&x.length<=128))].slice(0,6):[],size:typeof v.size==="number"&&Number.isFinite(v.size)?Math.max(24,Math.min(96,v.size)):48,minZoom:typeof v.minZoom==="number"&&Number.isFinite(v.minZoom)?Math.max(0,Math.min(22,v.minZoom)):14};
}
export function diagramValues(attrs:Record<string,unknown>,fields:readonly DiagramField[],config:DiagramSettings){
 const out=config.fields.flatMap((id,i)=>{
  const field=fields.find(f=>f.field_id===id&&["integer","decimal"].includes(f.type)),raw=Object.hasOwn(attrs,id)?attrs[id]:null;
  if(!field||raw===null||raw===undefined||typeof raw!=="number"&&(typeof raw!=="string"||!/^[-+]?(?:\d+(?:\.\d*)?|\.\d+)(?:[eE][-+]?\d+)?$/.test(raw)))return [];
  const value=Number(raw);return Number.isFinite(value)?[{id,label:field.name,value,color:diagramColors[i]!}]:[];
 });
 const max=Math.max(0,...out.map(v=>Math.abs(v.value)));
 return out.map(v=>({...v,fraction:max>0?v.value/max:0}));
}
export function featureAction(kind:"id"|"attributes",id:string,attrs:Record<string,unknown>){
 if(kind==="id")return id;
 const value=JSON.stringify(attrs,null,2);if(value.length>512000)throw Error("Thuộc tính vượt giới hạn sao chép.");return value;
}
