import {parseClassicNetcdf} from "./netcdf-classic";
import type {LocalMesh} from "./local-mesh";
import type {MeshDataset} from "./mesh-dataset";
export function loadNetcdf(bytes:ArrayBuffer,mesh:LocalMesh,variable:string,componentY:string,signal:AbortSignal):Promise<MeshDataset>{
 if(signal.aborted)return Promise.reject(signal.reason);
 if(bytes.byteLength>=4&&new DataView(bytes).getUint32(0)>>>8===0x434446){
  if(componentY.trim())return Promise.reject(Error("Classic NetCDF currently supports scalar fields only"));
  return Promise.resolve(parseClassicNetcdf(bytes,mesh,variable));
 }
 return new Promise((resolve,reject)=>{
  const worker=new Worker(new URL("./netcdf.worker.ts",import.meta.url),{type:"module"});
  let settled=false;const finish=(error:unknown,dataset?:MeshDataset)=>{if(settled)return;settled=true;clearTimeout(timer);signal.removeEventListener("abort",abort);worker.terminate();if(dataset)resolve(dataset);else reject(error)};
  const abort=()=>finish(Error("NetCDF decoding cancelled")),timer=setTimeout(()=>finish(Error("NetCDF4 decoding timed out")),30000);
  signal.addEventListener("abort",abort,{once:true});worker.onmessage=e=>e.data?.ok===true?finish(null,e.data.dataset):finish(Error(String(e.data?.error??"NetCDF4 decode failed")));worker.onerror=e=>finish(Error(e.message));worker.onmessageerror=()=>finish(Error("NetCDF4 worker response failed"));worker.postMessage({bytes,mesh,variable:variable.trim(),componentY:componentY.trim()||undefined},[bytes]);
 });
}
