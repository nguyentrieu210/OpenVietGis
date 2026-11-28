import type {LasCloud} from "./local-las";
export function loadLaz(bytes:ArrayBuffer,crs:4326|3857,signal:AbortSignal):Promise<LasCloud>{
 return new Promise((resolve,reject)=>{
  if(signal.aborted){reject(signal.reason);return}
  const worker=new Worker(new URL("./laz.worker.ts",import.meta.url),{type:"module"});
  let settled=false;const finish=(error:unknown,cloud?:LasCloud)=>{if(settled)return;settled=true;window.clearTimeout(timer);signal.removeEventListener("abort",abort);worker.terminate();if(cloud)resolve(cloud);else reject(error)};
  const abort=()=>finish(Error("Đã hủy đọc LAZ"));const timer=window.setTimeout(()=>finish(Error("LAZ decoder timeout")),30000);
  signal.addEventListener("abort",abort,{once:true});worker.onmessage=e=>{if(e.data?.ok===true&&e.data.cloud)finish(null,e.data.cloud);else finish(Error(String(e.data?.error??"LAZ decoder failed")))};
  worker.onerror=e=>finish(Error(e.message||"LAZ worker failed"));worker.onmessageerror=()=>finish(Error("LAZ worker message failed"));
  worker.postMessage({bytes,crs},[bytes]);
 });
}
