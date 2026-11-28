import type {Position} from "geojson";
import type {PathMetrics} from "./geom-measure";
type Kernel=WebAssembly.Exports&{memory:WebAssembly.Memory;alloc_path:(n:number)=>number;free_path:(ptr:number,n:number)=>void;path_metrics:(ptr:number,n:number)=>number};
export async function createWasmMetrics(bytes:ArrayBuffer|Uint8Array):Promise<PathMetrics>{
 const {instance}=await WebAssembly.instantiate(bytes as BufferSource);
 const wasm=instance.exports as Kernel;
 if(!(wasm.memory instanceof WebAssembly.Memory)||typeof wasm.alloc_path!=="function"||typeof wasm.free_path!=="function"||typeof wasm.path_metrics!=="function")throw Error("Invalid geometry WASM ABI");
 const cache=new WeakMap<Position[],{length:number;area:number}>();
 const calculate=(coords:Position[])=>{
  const cached=cache.get(coords);if(cached)return cached;
  const n=coords.length;if(n>2000000||coords.some(p=>!Number.isFinite(p[0])||!Number.isFinite(p[1])||Math.abs(p[0])>180||Math.abs(p[1])>90))throw Error("Unsupported geometry coordinates");
  const ptr=wasm.alloc_path(n);if(!ptr)throw Error("Geometry WASM allocation");
  try{
   const data=new Float64Array(wasm.memory.buffer,ptr,n*2+2);
   for(let i=0;i<n;i++){data[2*i]=coords[i][0];data[2*i+1]=coords[i][1]}
   if(wasm.path_metrics(ptr,n)!==1)throw Error("Geometry WASM failed");
   const value={length:data[n*2],area:data[n*2+1]};if(!Number.isFinite(value.length)||!Number.isFinite(value.area))throw Error("Geometry WASM nonfinite output");
   cache.set(coords,value);return value;
  }finally{wasm.free_path(ptr,n)}
 };
 return {length:c=>calculate(c).length,ringArea:c=>calculate(c).area};
}
