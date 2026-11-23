import {useState,useEffect,useRef} from "react";
import {createRoot} from "react-dom/client";
import {ApiClient,type SpatialLayer} from "../../../client-sdk/src/index";
import {TilesetPanel,type TilesetRecord} from "../../../../apps/web-next/src/TilesetPanel";
import {configureBrowserTestWorker} from "./worker";
import {parseTileset} from "../../src/tileset";
import {prepareTiles3d} from "../../src/tiles3d";
import type {SelectableTileEngine} from "../../src/tiles3d";
import {TileFeaturePanel,parseTileFeature,type TileFeature} from "../../../../apps/web-next/src/TileFeaturePanel";
configureBrowserTestWorker();
const client=new ApiClient({baseUrl:"/fixture"});
function Fixture(){
 const [layers,setLayers]=useState<SpatialLayer[]>([]),[selected,setSelected]=useState<TilesetRecord|null>(null),[busy,setBusy]=useState(false),[status,setStatus]=useState("");
 const surface=useRef<HTMLDivElement>(null),engine=useRef<SelectableTileEngine|null>(null),pick=useRef<AbortController|null>(null);
 const [feature,setFeature]=useState<TileFeature|null>(null),[message,setMessage]=useState("");
 useEffect(()=>{void client.request<SpatialLayer>("/layer.json").then(l=>setLayers([l]))},[]);
 const show=async()=>{if(!selected||!surface.current)return;const prefix="/tilesets/"+selected.id,a=new AbortController();const root=parseTileset(await client.request(prefix+"/tileset.json"));engine.current=await prepareTiles3d(surface.current,root,a.signal,async(uri,signal)=>(await client.requestRaw(prefix+"/"+uri,{signal})).arrayBuffer(),setStatus,hit=>{pick.current?.abort();setFeature(null);setMessage("");if(!hit)return;const req=new AbortController();pick.current=req;void client.request<unknown>(prefix+"/feature?"+new URLSearchParams({uri:hit.uri,triangle:String(hit.triangle)}),{signal:req.signal}).then(raw=>{if(req.signal.aborted)return;const v=parseTileFeature(raw,selected,hit);if(engine.current?.highlightFeature(hit,v.first_triangle,v.triangle_count))setFeature(v)}).catch(()=>{if(!req.signal.aborted)setMessage("lookup failed")})});engine.current.activate()};
 return <><TilesetPanel client={client} path="/tilesets" layers={layers} selected={selected} onSelect={setSelected} canEdit enabled onBusy={setBusy} expire={()=>setStatus("expired")}/><button disabled={!selected||busy} onClick={()=>void show()}>Xem mô hình đã tạo</button><button onClick={()=>engine.current?.setLayerVisible("tileset",false)}>Hide</button><button onClick={()=>engine.current?.setLayerVisible("tileset",true)}>Show</button><button onClick={()=>{pick.current?.abort();engine.current?.dispose();setFeature(null);setStatus("disposed")}}>Dispose</button><div ref={surface} style={{position:"relative",width:900,height:600}}/><pre id="status">{status}</pre>{selected&&<TileFeaturePanel record={selected} feature={feature} message={message} busy={false} onNavigate={v=>setMessage(v==="gis"?"GIS navigation "+feature?.feature_id:"CAD navigation "+v.handle)}/>}</>;
}
createRoot(document.querySelector("#root")!).render(<Fixture/>);
