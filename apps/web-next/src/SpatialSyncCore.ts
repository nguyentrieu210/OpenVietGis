export const STUDIO_FEATURE_PAGE_SIZE = 1_000;
const STUDIO_FEATURE_PAGE_CAP = 50;

export function studioFeaturePageLimit(featureCount: number | null | undefined) {
  return Math.min(
    STUDIO_FEATURE_PAGE_CAP,
    Math.max(1, Math.ceil((featureCount ?? 10_000) / STUDIO_FEATURE_PAGE_SIZE)),
  );
}

// A bounded revision chain proves that every changed feature is accounted for.
// Never read historical attributes into the map: detail reads enforce current ACL and CRS.
export function revisionChain<T extends {id:string;base_content_revision:string;resulting_content_revision:string;operation_count:number;status:string}>(items:T[],from:string,to:string):T[]|null{
  if(from===to)return [];
  const chain:T[]=[];let current=to,total=0;
  const seen=new Set<string>();
  while(current!==from){
    const c=items.find(x=>x.resulting_content_revision===current&&x.status==="committed");
    if(!c||seen.has(c.id)||chain.length>=10||(total+=c.operation_count)>64)return null;
    chain.push(c);seen.add(c.id);current=c.base_content_revision;
  }
  return chain.reverse();
}

export async function parallelBounded<T>(items:readonly T[],concurrency:number,run:(item:T,index:number)=>Promise<void>){
  let next=0;
  await Promise.all(Array.from({length:Math.min(items.length,concurrency)},async()=>{
    while(next<items.length){const index=next++;await run(items[index]!,index);}
  }));
}


type RenderFeature={id?:string|number;geometry:unknown;properties:Record<string,unknown>|null};
export function renderDiff<T extends RenderFeature>(before:readonly T[],after:readonly T[]){
  const old=new Map(before.map(f=>[f.id,f])),next=new Map(after.map(f=>[f.id,f]));
  if(old.size!==before.length||next.size!==after.length||old.has(undefined)||next.has(undefined))return null;
  const remove:(string|number)[]=[],add:T[]=[],update:{id:string|number;newGeometry?:T["geometry"];removeProperties?:string[];addOrUpdateProperties?:{key:string;value:unknown}[]}[]=[];
  for(const id of old.keys())if(!next.has(id))remove.push(id!);
  for(const [id,f] of next){
    const previous=old.get(id);
    if(!previous){add.push(f);continue;}
    const geometryChanged=previous.geometry!==f.geometry&&JSON.stringify(previous.geometry)!==JSON.stringify(f.geometry);
    const propertiesChanged=JSON.stringify(previous.properties)!==JSON.stringify(f.properties);
    // Never removeAllProperties: geojson-vt 6.3.3 then returns {} and ignores addOrUpdateProperties, so the feature loses layerId and vanishes until reload.
    if(geometryChanged||propertiesChanged)update.push({id:id!,...(geometryChanged?{newGeometry:f.geometry}:{}),...(propertiesChanged?{removeProperties:Object.keys(previous.properties??{}).filter(key=>!(key in (f.properties??{}))),addOrUpdateProperties:Object.entries(f.properties??{}).map(([key,value])=>({key,value}))}:{})});
  }
  return {remove,add,update};
}
