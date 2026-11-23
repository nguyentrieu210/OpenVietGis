import {useState,useEffect,useRef} from "react";
import {createRoot} from "react-dom/client";
import {ApiClient,type SpatialLayer} from "../../../client-sdk/src/index";
import {TilesetPanel,type TilesetRecord} from "../../../../apps/web-next/src/TilesetPanel";
import {setWorkerUrl} from "maplibre-gl";
import workerUrl from "maplibre-gl/dist/maplibre-gl-worker.mjs?url";
import {parseTileset} from "../../src/tileset";
import {prepareTiles3d} from "../../src/tiles3d";
import type {SelectableFootprintEngine} from "../../src/maplibre3d";
setWorkerUrl(workerUrl);
const client=new ApiClient({baseUrl:"/fixture"});
function Fixture(){
 const [layers,setLayers]=useState<SpatialLayer[]>([]),[selected,setSelected]=useState<TilesetRecord|null>(null),[busy,setBusy]=useState(false),[status,setStatus]=useState("");
 const surface=useRef<HTMLDivElement>(null),engine=useRef<SelectableFootprintEngine|null>(null);
 useEffect(()=>{void client.request<SpatialLayer>("/layer.json").then(l=>setLayers([l]))},[]);
 const show=async()=>{if(!selected||!surface.current)return;const prefix="/tilesets/"+selected.id,a=new AbortController();const root=parseTileset(await client.request(prefix+"/tileset.json"));engine.current=await prepareTiles3d(surface.current,root,a.signal,async(uri,signal)=>(await client.requestRaw(prefix+"/"+uri,{signal})).arrayBuffer(),setStatus);engine.current.activate()};
 return <><TilesetPanel client={client} path="/tilesets" layers={layers} selected={selected} onSelect={setSelected} canEdit enabled onBusy={setBusy} expire={()=>setStatus("expired")}/><button disabled={!selected||busy} onClick={()=>void show()}>Xem mô hình đã tạo</button><button onClick={()=>{engine.current?.dispose();setStatus("disposed")}}>Dispose</button><div ref={surface} style={{position:"relative",width:900,height:600}}/><pre id="status">{status}</pre></>;
}
createRoot(document.querySelector("#root")!).render(<Fixture/>);
