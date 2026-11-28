import type {StudioRow} from "./studio-contract";
export type IdentifyHit={layerId:string;key:string};
export const identifyId=(h:IdentifyHit)=>JSON.stringify([h.layerId,h.key]);
export const IDENTIFY_LIMIT=100;
export function identifyHits(features:readonly {properties?:Record<string,unknown>|null}[]){
 const seen=new Set<string>(),hits:IdentifyHit[]=[];
 for(const f of features){const p=f.properties;if(!p||typeof p.layerId!=="string"||typeof p.key!=="string"||!p.layerId||!p.key||p.layerId.length>128||p.key.length>768)continue;const h={layerId:p.layerId,key:p.key},id=identifyId(h);if(seen.has(id))continue;seen.add(id);hits.push(h);if(hits.length>=IDENTIFY_LIMIT)break}
 return hits;
}
export function identifyRows(hits:readonly IdentifyHit[],rows:readonly StudioRow[]){
 const index=new Map(rows.filter(r=>r.layer.lifecycle==="active"&&r.layer.capabilities.some(c=>c.name==="feature_read"&&c.available&&c.actor_authorized&&c.technical_supported)).map(r=>[identifyId({layerId:r.layer.id,key:r.key}),r]));
 return hits.flatMap(h=>{const r=index.get(identifyId(h));return r?[r]:[]});
}
export function mapTipText(row:StudioRow,field:string){
 if(!field||!row.layer.source.schema.fields.some(f=>f.field_id===field)||!Object.hasOwn(row.feature.attributes,field))return "";
 const v=row.feature.attributes[field];return typeof v==="string"||typeof v==="number"||typeof v==="boolean"?String(v).trim().slice(0,300):"";
}
