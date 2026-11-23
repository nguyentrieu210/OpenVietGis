import {useState,useEffect} from "react";
import {createRoot} from "react-dom/client";
import {ApiClient,type SpatialLayer,type SpatialFeatureDetail} from "../../../client-sdk/src/index";
import {SpatialWorkspace} from "../../../../apps/web-next/src/SpatialWorkspace";
import {featureKey,type StudioRow} from "../../../../apps/web-next/src/studio-contract";
import type {SourceFeatureRef} from "../../../../apps/web-next/src/TileFeaturePanel";
import {configureBrowserTestWorker,type BrowserTestMap as MLMap} from "./worker";
configureBrowserTestWorker();
const client=new ApiClient({baseUrl:"/fixture"}),camera={resize(){}} as MLMap;
function Fixture(){
 const [layer,setLayer]=useState<SpatialLayer|null>(null),[rows,setRows]=useState<StudioRow[]>([]),[selected,setSelected]=useState(""),[canRead,setCanRead]=useState(true);
 useEffect(()=>{void Promise.all([client.request<SpatialLayer>("/layer.json"),client.request<SpatialFeatureDetail>("/feature.json")]).then(([l,d])=>{setLayer(l);const f=d.feature;setRows([{key:featureKey(l.source_id,f.feature_id),layer:l,feature:f,color:"#246",geometry:f.geometry?{type:f.geometry.type,coordinates:f.geometry.coordinates} as StudioRow["geometry"]:null}])})},[]);
 if(!layer)return <p>Loading fixture</p>;
 const row=rows.find(r=>r.key===selected);
 const open=async(source:SourceFeatureRef,signal:AbortSignal)=>{const path="/organizations/"+layer.organization_id+"/projects/"+layer.project_id+"/layers/"+layer.id+"/features/"+source.feature_id;const detail=await client.request<SpatialFeatureDetail>(path,{signal});if(signal.aborted||detail.feature.feature_id!==source.feature_id||detail.context.source_id!==source.source_id)return false;setSelected(featureKey(source.source_id,source.feature_id));return true};
 return <main style={{height:"94vh",display:"flex",flexDirection:"column"}}><button onClick={()=>setCanRead(false)}>Revoke access</button><SpatialWorkspace scope={{accountId:"fixture",organizationId:layer.organization_id,projectId:layer.project_id}} client={client} canRead={canRead} canEdit blocked={false} rows={rows} rasters={[]} camera={()=>camera} expire={()=>setCanRead(false)} onCadImport={()=>{}} selectedKey={selected} selectedSourceRow={row} onClearSelection={()=>setSelected("")} onSelectRow={setSelected} onOpenTileFeature={open}><section><button onClick={()=>setSelected(rows[0]!.key)}>Select GIS feature</button><p data-testid="selection">{selected||"none"}</p></section></SpatialWorkspace></main>;
}
createRoot(document.querySelector("#root")!).render(<Fixture/>);
