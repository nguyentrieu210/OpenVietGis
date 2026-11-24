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

