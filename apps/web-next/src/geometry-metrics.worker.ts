import {derive} from "./geom-measure";
import type {Geometry} from "geojson";
self.onmessage=(e:MessageEvent<Geometry>)=>{try{self.postMessage({ok:true,value:derive(e.data)})}catch{self.postMessage({ok:false})}};
