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
// The render pipeline recreates plain scalar style objects on each selection.
// Avoid allocating two JSON strings for every unchanged feature. Fall back to
// the previous serialization semantics for nested values or differing keys.
function sameRenderProperties(a:RenderFeature["properties"],b:RenderFeature["properties"]):boolean{
  if(a===b)return true;
  if(a&&b&&Object.getPrototypeOf(a)===Object.prototype&&Object.getPrototypeOf(b)===Object.prototype){
    const keys=Object.keys(a),other=Object.keys(b);
    if(keys.length===other.length&&keys.every((k,i)=>{
      const x=a[k];
      return k===other[i]&&(x===null||typeof x==="string"||typeof x==="number"||typeof x==="boolean")&&x===b[k];
    }))return true;
  }
  return JSON.stringify(a)===JSON.stringify(b);
}
export function renderDiff<T extends RenderFeature>(before:readonly T[],after:readonly T[]){
  const old=new Map(before.map(f=>[f.id,f])),next=new Map(after.map(f=>[f.id,f]));
  if(old.size!==before.length||next.size!==after.length||old.has(undefined)||next.has(undefined))return null;
  const remove:(string|number)[]=[],add:T[]=[],update:{id:string|number;newGeometry?:T["geometry"];removeProperties?:string[];addOrUpdateProperties?:{key:string;value:unknown}[]}[]=[];
  for(const id of old.keys())if(!next.has(id))remove.push(id!);
  for(const [id,f] of next){
    const previous=old.get(id);
    if(!previous){add.push(f);continue;}
    const geometryChanged=previous.geometry!==f.geometry&&JSON.stringify(previous.geometry)!==JSON.stringify(f.geometry);
    const propertiesChanged=!sameRenderProperties(previous.properties,f.properties);
    // Never removeAllProperties: geojson-vt 6.3.3 then returns {} and ignores addOrUpdateProperties, so the feature loses layerId and vanishes until reload.
    if(geometryChanged||propertiesChanged)update.push({id:id!,...(geometryChanged?{newGeometry:f.geometry}:{}),...(propertiesChanged?{removeProperties:Object.keys(previous.properties??{}).filter(key=>!(key in (f.properties??{}))),addOrUpdateProperties:Object.entries(f.properties??{}).map(([key,value])=>({key,value}))}:{})});
  }
  return {remove,add,update};
}

export type WakeTimer = ReturnType<typeof setTimeout>;
/**
 * Coalesce arbitrary realtime/focus/reconnect wakeups into at most one active
 * sync plus one pending rerun. The callback may request a delayed retry by
 * calling trigger again while it is running.
 */
export function createCoalescedWake(
  run:()=>Promise<void>,
  timers:{
    set:(callback:()=>void,delay:number)=>WakeTimer;
    clear:(timer:WakeTimer)=>void;
  }={set:(callback,delay)=>setTimeout(callback,delay),clear:timer=>clearTimeout(timer)}
){
  let stopped=false,running=false,pending=false,timer:WakeTimer|undefined;
  const drain=async()=>{
    if(stopped||running||!pending)return;
    pending=false;running=true;
    try{await run()}finally{
      running=false;
      if(pending&&!stopped&&timer===undefined)trigger();
    }
  };
  const trigger=(delay=250)=>{
    pending=true;
    if(stopped||running||timer!==undefined)return;
    timer=timers.set(()=>{timer=undefined;void drain()},Math.max(0,delay));
  };
  const stop=()=>{
    stopped=true;pending=false;
    if(timer!==undefined){timers.clear(timer);timer=undefined}
  };
  return {trigger,stop,isRunning:()=>running};
}
