import {setWorkerUrl} from "maplibre-gl";import workerUrl from "maplibre-gl/dist/maplibre-gl-worker.mjs?url";
import {prepareTiles3d} from "../../src/tiles3d";import {parseTileset} from "../../src/tileset";import {glbFixture} from "../terrain-model-fixture.mjs";
setWorkerUrl(workerUrl);
const region=[-.00003,-.00003,.00003,.00003,-1,50],transform=[0,1,0,0,0,0,1,0,1,0,0,0,6378137,0,0,1];
const root=parseTileset({asset:{version:"1.1"},geometricError:20,root:{boundingVolume:{region},geometricError:10,refine:"REPLACE",transform,content:{uri:"coarse.glb"},children:[{boundingVolume:{region},geometricError:0,content:{uri:"detail.glb"}}]}});
const status=document.querySelector("#status")!,requests=document.querySelector("#requests")!,a=new AbortController();
prepareTiles3d(document.querySelector<HTMLElement>("#surface")!,root,a.signal,async(uri)=>{requests.textContent+=uri+" ";return glbFixture()},v=>status.textContent=v).then(engine=>{engine.activate();document.querySelector("#dispose")!.addEventListener("click",()=>{engine.dispose();status.textContent="disposed"})}).catch(e=>{status.textContent=String(e);console.error(e)});
