import {useEffect,useRef,useState} from "react";
import {createRoot} from "react-dom/client";
import {Workbench,type PanelSpec} from "@openvietgis/ui-react";
import "@openvietgis/ui-react/styles.css";
import "../../packages/design-tokens/generated/tokens.css";
import {Map as MapSurface,configureMapSurface} from "./src/map-surface";
configureMapSurface();
let serial=0;
function RealMap(){
 const el=useRef<HTMLDivElement>(null),map=useRef<MapSurface|null>(null),[ready,setReady]=useState(false),[mount]=useState(()=>++serial),[state,setState]=useState("");
 useEffect(()=>{
  if(!el.current)return;
  const m=new MapSurface({container:el.current,center:[106,11],zoom:8,attributionControl:false,style:{version:8,sources:{test:{type:"geojson",data:{type:"FeatureCollection",features:[{type:"Feature",id:1,geometry:{type:"Point",coordinates:[106.6,11.1]},properties:{}}]}}},layers:[{id:"background",type:"background",paint:{"background-color":"#cbd5df"}},{id:"point",type:"circle",source:"test",paint:{"circle-radius":10,"circle-color":["case",["boolean",["feature-state","selected"],false],"#ff0000","#1565a9"]}}]}});
  map.current=m;m.once("load",()=>setReady(true));const observer=new ResizeObserver(()=>{if(el.current?.clientWidth&&el.current?.clientHeight)m.resize()});observer.observe(el.current);
  return()=>{observer.disconnect();map.current=null;m.remove()};
 },[]);
 const snapshot=()=>{const m=map.current;if(m)setState(JSON.stringify({x:Number(m.getCenter().lng.toFixed(3)),y:Number(m.getCenter().lat.toFixed(3)),zoom:Number(m.getZoom().toFixed(2)),selected:m.getFeatureState({source:"test",id:1}).selected===true}))};
 return <div data-testid="real-map" data-mount={mount} style={{height:"100%",display:"flex",flexDirection:"column"}}><div><button disabled={!ready} onClick={()=>{map.current!.jumpTo({center:[106.6,11.1],zoom:12});map.current!.setFeatureState({source:"test",id:1},{selected:true});snapshot()}}>Set map state</button><button disabled={!ready} onClick={snapshot}>Read map state</button><output data-testid="map-state">{state}</output></div><div ref={el} style={{flex:1,minHeight:0}}/></div>;
}
const Other=()=> <p>Other panel</p>;
const panels:PanelSpec[]=[{id:"map",title:"Map",render:RealMap,persistent:true},{id:"layers",title:"Layers",render:Other},{id:"inspector",title:"Inspector",render:Other}];
createRoot(document.getElementById("root")!).render(<div style={{height:"98vh"}}><Workbench scope={{account:"fixture",project:"map",workspace:"dock"}} panels={panels}/></div>);