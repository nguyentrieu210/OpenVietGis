import {configureBrowserTestWorker,prepareFootprint3d} from "./worker";
import {footprint3d} from "../../src/footprint3d";
import {parseStaticModel} from "../../src/gltf";
import {validateTerrain,geographic3857,sampleTerrain} from "../../src/terrain";
import {glbFixture,terrainFixture} from "../terrain-model-fixture.mjs";
configureBrowserTestWorker();
const grid=validateTerrain(terrainFixture(),"dem"),[l,b,r,t]=grid.extent,center=geographic3857((l+r)/2,(b+t)/2);
const a=geographic3857(l+20,t-20),b2=geographic3857(l+75,t-20);
const model=parseStaticModel(glbFixture(),"glb"),colors=new Float32Array(model.colors.length);
for(let i=0;i<colors.length;i+=3){colors[i]=.02;colors[i+1]=.08;colors[i+2]=.9}
const status=document.querySelector("#status")!,surface=document.querySelector<HTMLElement>("#surface")!,picked=document.querySelector("#picked")!,abort=new AbortController();
prepareFootprint3d(surface,footprint3d([],"EPSG:4326","relative-terrain-reference"),{longitude:center[0],latitude:center[1],zoom:19,bearing:0},abort.signal,()=>{},{
 terrain:grid,models:[{id:"red",placement:{model,longitude:a[0],latitude:a[1],altitude:sampleTerrain(grid,...a)!-grid.render_origin_m,rotationDeg:0,scale:.8}},{id:"blue",placement:{model:{...model,colors},longitude:b2[0],latitude:b2[1],altitude:sampleTerrain(grid,...b2)!-grid.render_origin_m,rotationDeg:0,scale:.8}}],
 onModelPick(id){picked.textContent=id}
}).then(engine=>{engine.activate();status.textContent="scene ready";document.querySelector("#hide")!.addEventListener("click",()=>{engine.setLayerVisible("blue",false);engine.selectModel("");picked.textContent="none"});document.querySelector("#show")!.addEventListener("click",()=>engine.setLayerVisible("blue",true));document.querySelector("#dispose")!.addEventListener("click",()=>{engine.dispose();status.textContent="disposed"})}).catch(e=>{status.textContent="scene failed: "+String(e);console.error(e)});
