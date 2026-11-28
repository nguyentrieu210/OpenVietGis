import {derive} from "./geom-measure";
import type {Geometry} from "geojson";
import {createWasmMetrics} from "./geometry-wasm";
import wasmUrl from "./wasm/geometry.wasm?url";
let kernel:ReturnType<typeof createWasmMetrics>|undefined;
self.onmessage=async(e:MessageEvent<Geometry>)=>{
 try{
  kernel??=fetch(wasmUrl).then(r=>{if(!r.ok)throw Error("WASM fetch");return r.arrayBuffer()}).then(createWasmMetrics);
  let value,engine="wasm";try{value=derive(e.data,await kernel)}catch{engine="js";value=derive(e.data)}
  self.postMessage({ok:true,value,engine});
 }catch{self.postMessage({ok:false})}
};
