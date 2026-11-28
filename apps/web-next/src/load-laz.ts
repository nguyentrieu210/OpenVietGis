import type {CloudRegion} from "./cloud-region";
import type {LasCloud} from "./local-las";
import type {CopcPreview} from "./remote-copc";
type Request={kind:"file";bytes:ArrayBuffer;crs:4326|3857}|{kind:"copc";url:string;depth:number;crs:4326|3857;region?:CloudRegion};
function request<T extends LasCloud>(message:Request,signal:AbortSignal):Promise<T>{
 return new Promise((resolve,reject)=>{
  if(signal.aborted){reject(signal.reason);return}
  const worker=new Worker(new URL("./laz.worker.ts",import.meta.url),{type:"module"});
  let settled=false;const finish=(error:unknown,cloud?:T)=>{if(settled)return;settled=true;window.clearTimeout(timer);signal.removeEventListener("abort",abort);worker.terminate();if(cloud)resolve(cloud);else reject(error)};
  const abort=()=>finish(Error("Đã hủy đọc LAZ"));const timer=window.setTimeout(()=>finish(Error("LAZ decoder timeout")),30000);
  signal.addEventListener("abort",abort,{once:true});worker.onmessage=e=>{if(e.data?.ok===true&&e.data.cloud)finish(null,e.data.cloud);else finish(Error(String(e.data?.error??"LAZ decoder failed")))};
  worker.onerror=e=>finish(Error(e.message||"LAZ worker failed"));worker.onmessageerror=()=>finish(Error("LAZ worker message failed"));
  worker.postMessage(message,message.kind==="file"?[message.bytes]:[]);
 });
}

export function loadLaz(bytes:ArrayBuffer,crs:4326|3857,signal:AbortSignal){return request<LasCloud>({kind:"file",bytes,crs},signal)}
export function loadRemoteCopc(url:string,crs:4326|3857,depth:number,signal:AbortSignal,region?:CloudRegion){return request<CopcPreview>({kind:"copc",url,crs,depth,region},signal)}
