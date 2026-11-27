import React,{useEffect,useRef,useState} from "react";
import type {Feature,FeatureCollection,Geometry,Position} from "geojson";
import * as ml from "./map-surface";
import {Brand} from "./Brand";
import {derive,fmtArea,fmtLength} from "./geom-measure";
import {getBasemap} from "./Basemaps";
import {appPath} from "./Landing";
import {parseGuestGeoJson,validPosition} from "./guest-geojson";
import "./guest-studio.css";

const KEY="ovg:guest:geojson:v1";
const CAMERA="ovg:guest:camera:v1";
const empty=():FeatureCollection=>({type:"FeatureCollection",features:[]});
type Tool="pan"|"Point"|"LineString"|"Polygon";
function load():FeatureCollection{try{return parseGuestGeoJson(JSON.parse(localStorage.getItem(KEY)??"null"))}catch{return empty()}}
function readCamera():{center:[number,number];zoom:number}{try{const x=JSON.parse(localStorage.getItem(CAMERA)??"null");if(x&&validPosition(x.center)&&x.center.length===2&&Number.isFinite(x.zoom)&&x.zoom>=0&&x.zoom<=22)return x}catch{}return {center:[106.4,14.7],zoom:5}}
export function GuestStudio({embedded=false,checking=false,onSave,error,authenticated=false}:{embedded?:boolean;checking?:boolean;onSave?:()=>void;error?:string;authenticated?:boolean}){
 const [data,setData]=useState(load),[tool,setTool]=useState<Tool>("pan"),[draft,setDraft]=useState<Position[]>([]),[notice,setNotice]=useState(""),[ready,setReady]=useState(false),[visible,setVisible]=useState(true);
 const host=useRef<HTMLDivElement>(null),map=useRef<ml.Map|null>(null),file=useRef<HTMLInputElement>(null),toolRef=useRef(tool);
 toolRef.current=tool;
 const base=getBasemap("terrain");
 useEffect(()=>{
  if(!host.current)return;
  let m:ml.Map;
  try{m=new ml.Map({container:host.current,...readCamera(),style:{version:8,sources:{base:{type:"raster",tiles:[base.tiles],tileSize:256,maxzoom:base.maxzoom,attribution:base.attribution}},layers:[{id:"base",type:"raster",source:"base"}]}})}catch{setNotice("Không mở được bản đồ WebGL trên thiết bị này.");return}
  map.current=m;m.addControl(new ml.NavigationControl(),"top-right");m.addControl(new ml.ScaleControl(),"bottom-left");
  m.on("load",()=>{m.addSource("guest",{type:"geojson",data:empty()});m.addLayer({id:"guest-fill",type:"fill",source:"guest",filter:["==","$type","Polygon"],paint:{"fill-color":"#1684c2","fill-opacity":0.3}});m.addLayer({id:"guest-line",type:"line",source:"guest",filter:["!=","$type","Point"],paint:{"line-color":"#1769aa","line-width":3}});m.addLayer({id:"guest-point",type:"circle",source:"guest",filter:["==","$type","Point"],paint:{"circle-color":"#e36a2e","circle-radius":5,"circle-stroke-width":1,"circle-stroke-color":"#fff"}});setReady(true)});
  m.on("click",e=>{const p=[e.lngLat.wrap().lng,e.lngLat.lat];if(toolRef.current==="Point")setData(v=>v.features.length>=10000?v:({...v,features:[...v.features,{type:"Feature",geometry:{type:"Point",coordinates:p},properties:{}}]}));else if(toolRef.current!=="pan")setDraft(v=>[...v,p])});
  m.on("moveend",()=>{try{localStorage.setItem(CAMERA,JSON.stringify({center:m.getCenter().toArray(),zoom:m.getZoom()}))}catch{}});
  return()=>{map.current=null;m.remove()};
 },[base]);
 useEffect(()=>{try{localStorage.setItem(KEY,JSON.stringify(data))}catch{setNotice("Bộ nhớ trình duyệt không đủ. Hãy tải GeoJSON về máy.")}},[data]);
 useEffect(()=>{if(!ready||!map.current)return;const preview:Feature[]=draft.length>1?[{type:"Feature",geometry:{type:"LineString",coordinates:draft},properties:{}}]:draft.length?[{type:"Feature",geometry:{type:"Point",coordinates:draft[0]!},properties:{}}]:[];(map.current.getSource("guest") as ml.GeoJSONSource|undefined)?.setData({type:"FeatureCollection",features:[...(visible?data.features:[]),...preview]});},[ready,data,draft,visible]);
 useEffect(()=>{if(map.current)map.current.getCanvas().style.cursor=tool==="pan"?"":"crosshair"},[tool,ready]);
 const finish=()=>{if(data.features.length>=10000){setNotice("Tối đa 10.000 đối tượng.");return}if(tool==="LineString"&&draft.length>=2||tool==="Polygon"&&draft.length>=3){const geometry:Geometry=tool==="LineString"?{type:"LineString",coordinates:draft}:{type:"Polygon",coordinates:[[...draft,draft[0]!]]};setData(v=>({...v,features:[...v.features,{type:"Feature",geometry,properties:{}}]}));const d=derive(geometry);setNotice(d.area!==undefined?fmtArea(d.area):fmtLength(d.length??0));setDraft([]);setTool("pan")}};
 const importFile=async(f:File|undefined)=>{if(!f)return;try{if(f.size>10*1024*1024)throw Error("Tệp vượt quá 10 MB.");const v=parseGuestGeoJson(JSON.parse(await f.text()));setData(old=>{if(old.features.length+v.features.length>10000){setNotice("Tối đa 10.000 đối tượng.");return old}return {...old,features:[...old.features,...v.features]}});setVisible(true);setNotice("Đã đọc "+v.features.length+" đối tượng GeoJSON.");}catch(e){setNotice(e instanceof Error?e.message:"Không đọc được tệp.")}finally{if(file.current)file.current.value=""}};
 const download=()=>{const url=URL.createObjectURL(new Blob([JSON.stringify(data)],{type:"application/geo+json"}));const a=document.createElement("a");a.href=url;a.download="openvietgis-guest.geojson";a.click();setTimeout(()=>URL.revokeObjectURL(url),1000)};
 const authPath=appPath.startsWith("/devui/")?"/devui/":"/";
 return <section className={"ovg-guest "+(embedded?"embedded":"")} aria-label="GIS dùng thử">
  {!embedded&&<header className="guest-top"><Brand/><b>Không gian GIS cá nhân</b><span className="guest-local">{checking?"Đang kiểm tra phiên":"Khách · dữ liệu cục bộ"}</span><a href={authPath+"?about=1"}>Giới thiệu</a>{authenticated?<a className="guest-primary" href={appPath}>Mở dự án Cloud</a>:<><a href={authPath+"?login=1"}>Đăng nhập</a><a className="guest-primary" href={authPath+"?register=1"}>Đăng ký miễn phí</a></>}</header>}
  <div className="guest-tools" role="toolbar" aria-label="Công cụ bản đồ">{([["pan","Di chuyển"],["Point","Vẽ điểm"],["LineString","Vẽ đường"],["Polygon","Vẽ vùng"]] as const).map(([id,label])=><button key={id} aria-pressed={tool===id} onClick={()=>{setDraft([]);setTool(id)}}>{label}</button>)}<button disabled={!(tool==="LineString"&&draft.length>=2||tool==="Polygon"&&draft.length>=3)} onClick={finish}>Hoàn tất</button><button disabled={!draft.length} onClick={()=>setDraft(v=>v.slice(0,-1))}>Bỏ đỉnh</button><button onClick={()=>file.current?.click()}>Nhập GeoJSON</button><input hidden ref={file} type="file" accept=".geojson,.json" onChange={e=>void importFile(e.target.files?.[0])}/><button onClick={download} disabled={!data.features.length}>Xuất GeoJSON</button>{embedded&&<button className="guest-primary" onClick={onSave}>Tạo không gian Cloud</button>}</div>
  <div className="guest-body"><aside><h3>Lớp dữ liệu</h3><label><input type="checkbox" checked={visible} onChange={e=>setVisible(e.target.checked)}/> Đối tượng cục bộ ({data.features.length})</label><button disabled={!data.features.length} onClick={()=>setData(v=>({...v,features:v.features.slice(0,-1)}))}>Bỏ đối tượng cuối</button><p>Vẽ trực tiếp hoặc nhập GeoJSON. Dữ liệu chỉ lưu trong trình duyệt này.</p><p>Muốn lưu dự án Cloud hoặc cộng tác, hãy đăng nhập và chọn không gian làm việc. Có thể xuất GeoJSON để chuyển dữ liệu.</p>{error&&<p role="alert">{error}</p>}</aside><div className="guest-map" ref={host}/></div>
  <footer role="status">WGS 84 · EPSG:4326 · {notice||"Sẵn sàng trải nghiệm GIS, không cần tài khoản."}</footer>
 </section>
}
