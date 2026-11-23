import {setWorkerUrl} from "maplibre-gl";
import workerUrl from "maplibre-gl/dist/maplibre-gl-worker.mjs?url";
import {prepareFootprint3d} from "../../src/maplibre3d";
import {footprint3d} from "../../src/footprint3d";
import {parseStaticModel} from "../../src/gltf";
import {validateTerrain,geographic3857,sampleTerrain} from "../../src/terrain";
import {glbFixture,terrainFixture} from "../terrain-model-fixture.mjs";
setWorkerUrl(workerUrl);
const grid=validateTerrain(terrainFixture(),"dem"),[l,b,r,t]=grid.extent,center=geographic3857((l+r)/2,(b+t)/2);
const anchor=geographic3857(l+30,t-35),ground=sampleTerrain(grid,...anchor)!;
const status=document.querySelector("#status")!,surface=document.querySelector<HTMLElement>("#surface")!,abort=new AbortController();
const model=parseStaticModel(glbFixture(),"glb");
prepareFootprint3d(surface,footprint3d([],"EPSG:4326","relative-terrain-reference"),{longitude:center[0],latitude:center[1],zoom:19,bearing:20},abort.signal,()=>{},{
 terrain:grid,model:{model,longitude:anchor[0],latitude:anchor[1],altitude:ground-grid.render_origin_m,rotationDeg:20,scale:1}
}).then(engine=>{engine.activate();status.textContent="scene ready";const btn=document.createElement("button");btn.textContent="Dispose";btn.onclick=()=>{engine.dispose();status.textContent="disposed"};document.body.append(btn)}).catch(e=>{console.error(e);status.textContent="scene failed: "+String(e)});
