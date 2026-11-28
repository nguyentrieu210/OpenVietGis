import {useEffect,useState} from "react";
import type {Geometry} from "geojson";
import {derive,type Derived} from "./geom-measure";
const cache=new WeakMap<Geometry,Derived>();
export function useGeometryMetrics(geometry:Geometry|null|undefined){
 const [result,setResult]=useState<{geometry:Geometry;value:Derived}|null>(null);
 useEffect(()=>{
  if(!geometry||cache.has(geometry))return;
  let active=true,worker:Worker|undefined;
  const finish=(value:Derived)=>{if(active){cache.set(geometry,value);setResult({geometry,value})}};
  const fallback=()=>{worker?.terminate();worker=undefined;if(active)finish(derive(geometry))};
  try{worker=new Worker(new URL("./geometry-metrics.worker.ts",import.meta.url),{type:"module"});worker.onmessage=e=>{if(e.data?.ok){finish(e.data.value);worker?.terminate();worker=undefined}else fallback()};worker.onerror=fallback;worker.postMessage(geometry)}catch{fallback()}
  return()=>{active=false;worker?.terminate()};
 },[geometry]);
 return geometry?(cache.get(geometry)??(result?.geometry===geometry?result.value:null)):null;
}
