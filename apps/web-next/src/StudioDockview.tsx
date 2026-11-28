import {createContext,useContext,useEffect,useState,type ReactNode,type ComponentProps} from "react";
import {Workbench,type PanelSpec,type Scope} from "@openvietgis/ui-react/workbench";
import "@openvietgis/ui-react/styles.css";
import "./studio-dockview.css";
type PanelId="map"|"layers"|"inspector"|"attributes";
type Api=Parameters<NonNullable<ComponentProps<typeof Workbench>["onReady"]>>[0];
type Slots=Record<PanelId,ReactNode>;
const SlotsContext=createContext<Slots|null>(null);
const Slot=({id}:{id:PanelId})=>{const slots=useContext(SlotsContext);return <div className="ovg-studio-dock-slot" data-slot={id}>{slots?.[id]}</div>};
const MapSlot=()=> <Slot id="map"/>, LayersSlot=()=> <Slot id="layers"/>, InspectorSlot=()=> <Slot id="inspector"/>, AttributesSlot=()=> <Slot id="attributes"/>;
const panels:PanelSpec[]=[{id:"map",title:"Bản đồ",render:MapSlot,persistent:true},{id:"layers",title:"Lớp",render:LayersSlot},{id:"inspector",title:"Thông tin",render:InspectorSlot},{id:"attributes",title:"Thuộc tính",render:AttributesSlot}];
export default function StudioDockview({scope,slots,blocked,focus}:{scope:Scope;slots:Slots;blocked:boolean;focus:boolean}){
 const [api,setApi]=useState<Api|null>(null);
 const layers=!!slots.layers,inspector=!!slots.inspector,attributes=!!slots.attributes;
 useEffect(()=>{
  if(!api)return;
  for(const [id,visible,direction] of [["layers",layers,"left"],["inspector",inspector,"right"],["attributes",attributes,"below"]] as const){
   const p=api.getPanel(id);
   if(!visible&&p)api.removePanel(p);
   if(visible&&!p){const spec=panels.find(s=>s.id===id)!;api.addPanel({id,component:id,title:spec.title,renderer:"always",position:{referencePanel:"map",direction},initialWidth:id==="layers"?250:300,initialHeight:220,minimumWidth:150,minimumHeight:100})}
  }
 },[api,layers,inspector,attributes]);
 useEffect(()=>{const map=api?.getPanel("map");if(!map)return;if(focus&&!map.api.isMaximized())map.api.maximize();else if(!focus&&map.api.isMaximized())map.api.exitMaximized()},[api,focus]);
 const fallback=()=>{if(blocked)return;const url=new URL(location.href);url.searchParams.delete("dockview");location.assign(url.href)};
 return <SlotsContext.Provider value={slots}><div className="ovg-studio-dockview" data-testid="studio-dockview"><Workbench scope={scope} panels={panels} preset="map" onReady={setApi} managed locked={blocked} visiblePanels={["map",...(layers?["layers"]:[]),...(inspector?["inspector"]:[]),...(attributes?["attributes"]:[])]}><button type="button" disabled={blocked} onClick={fallback}>Bố cục cổ điển</button></Workbench></div></SlotsContext.Provider>;
}