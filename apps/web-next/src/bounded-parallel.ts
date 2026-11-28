/** Bounded concurrency preserving input order. Stop scheduling on first
 * failure and join all outstanding operations before returning the error. */
export async function boundedParallel<T,R>(items:readonly T[],limit:number,run:(item:T,index:number)=>Promise<R>,signal:AbortSignal):Promise<R[]>{
 if(!Number.isInteger(limit)||limit<1||limit>8)throw Error("Invalid concurrency");
 const out=new Array<R>(items.length);let cursor=0,failed=false,problem:unknown;
 const worker=async()=>{while(!failed&&cursor<items.length){
  const i=cursor++;
  try{signal.throwIfAborted();out[i]=await run(items[i]!,i)}catch(e){if(!failed){failed=true;problem=e}}
 }};
 await Promise.all(Array.from({length:Math.min(limit,items.length)},worker));
 if(failed)throw problem;signal.throwIfAborted();return out;
}
