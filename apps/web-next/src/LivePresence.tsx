import {useEffect,useRef,useState,type RefObject} from "react";
import * as ml from "./map-surface";
import "./live-presence.css";
type Peer={id:string;actor:string;name:string;color:string;cursor:number[]|null;view:number[]|null};
type Props={org:string;pid:string;map:RefObject<ml.Map|null>;blocked:boolean;preserveView:()=>void};
export function LivePresence({org,pid,map,blocked,preserveView}:Props){
 const [enabled,setEnabled]=useState(false),[status,setStatus]=useState(""),[peers,setPeers]=useState<Peer[]>([]),[self,setSelf]=useState(""),[follow,setFollow]=useState("");
 const followRef=useRef(follow),blockedRef=useRef(blocked),preserve=useRef(preserveView);followRef.current=follow;blockedRef.current=blocked;preserve.current=preserveView;
 useEffect(()=>{if(blocked)setFollow("")},[blocked]);
 useEffect(()=>{
  if(!enabled||!map.current)return;
  const m=map.current,markers=new Map<string,ml.Marker>();let stopped=false,ws:WebSocket|null=null,retry:ReturnType<typeof setTimeout>|undefined,timer:ReturnType<typeof setTimeout>|undefined,delay=1000,last=0,pending:Record<string,unknown>={},me="";
  const clear=()=>{markers.forEach(x=>x.remove());markers.clear();setPeers([]);setSelf("");setFollow("")};
  const flush=()=>{timer=undefined;if(ws?.readyState!==WebSocket.OPEN)return;if(Object.keys(pending).length&&ws.bufferedAmount<16384){ws.send(JSON.stringify(pending));pending={};last=Date.now()}};
  const send=(patch:Record<string,unknown>)=>{Object.assign(pending,patch);if(timer===undefined)timer=setTimeout(flush,Math.max(0,110-(Date.now()-last)))};
  const view=()=>{if(followRef.current)return;const c=m.getCenter().wrap();send({view:[c.lng,Math.max(-85,Math.min(85,c.lat)),Math.max(0,Math.min(24,m.getZoom())),m.getBearing(),m.getPitch()]})};
  const move=(e:ml.MapMouseEvent)=>{const c=e.lngLat.wrap();send({cursor:[c.lng,c.lat]})};
  const leave=()=>send({cursor:null});
  const manual=()=>{followRef.current="";setFollow("")};
  const connect=()=>{
   if(stopped)return;
   setStatus("Đang kết nối…");
   const socket=new WebSocket((location.protocol==="https:"?"wss:":"ws:")+"//"+location.host+"/api/v1/organizations/"+org+"/projects/"+pid+"/collaboration/presence");ws=socket;
   socket.onopen=()=>{if(stopped)return;delay=1000;setStatus("Trực tuyến");view()};
   socket.onmessage=e=>{if(stopped)return;try{
    const packet=JSON.parse(e.data);if(packet.type!=="presence"||!Array.isArray(packet.users))return;
    const users:Peer[]=packet.users;me=packet.self_id;setSelf(me);setPeers(users);
    const visible=new Set<string>();
    for(const p of users){if(p.id===me||!p.cursor)continue;visible.add(p.id);let marker=markers.get(p.id);
     if(!marker){const el=document.createElement("div");el.className="live-cursor";el.style.setProperty("--peer-color",p.color);const arrow=document.createElement("span");arrow.textContent="➤";const label=document.createElement("b");label.textContent=p.name;el.append(arrow,label);marker=new ml.Marker({element:el,anchor:"top-left"}).setLngLat([p.cursor[0],p.cursor[1]]).addTo(m);markers.set(p.id,marker)}else marker.setLngLat([p.cursor[0],p.cursor[1]])}
    for(const [id,marker] of markers)if(!visible.has(id)){marker.remove();markers.delete(id)}
    const target=users.find(p=>p.id===followRef.current);if(followRef.current&&!target)setFollow("");
    if(target?.view&&!blockedRef.current){const [lng,lat,zoom,bearing,pitch]=target.view;const c=m.getCenter().wrap();if(Math.abs(c.lng-lng)+Math.abs(c.lat-lat)>0.000001||Math.abs(m.getZoom()-zoom)>0.0001||Math.abs(m.getBearing()-bearing)>0.01||Math.abs(m.getPitch()-pitch)>0.01){preserve.current();m.jumpTo({center:[lng,lat],zoom,bearing,pitch})}}
   }catch{/* ignore malformed frames */}};
   socket.onclose=e=>{if(stopped)return;clear();pending={};if(e.code===1008){setStatus("Kết nối không được phép");setEnabled(false);return}setStatus("Mất kết nối · thử lại");retry=setTimeout(connect,delay);delay=Math.min(15000,delay*2)};
  };
  m.on("mousemove",move);m.on("moveend",view);m.getCanvas().addEventListener("mouseleave",leave);m.getCanvas().addEventListener("pointerdown",manual);m.getCanvas().addEventListener("wheel",manual);connect();
  return()=>{stopped=true;clearTimeout(retry);clearTimeout(timer);ws?.close();m.off("mousemove",move);m.off("moveend",view);m.getCanvas().removeEventListener("mouseleave",leave);m.getCanvas().removeEventListener("pointerdown",manual);m.getCanvas().removeEventListener("wheel",manual);clear()};
 },[enabled,org,pid,map]);
 function choose(p:Peer){if(blocked||!p.view||!map.current)return;const next=follow===p.id?"":p.id;followRef.current=next;setFollow(next);if(next){const [lng,lat,zoom,bearing,pitch]=p.view;preserveView();map.current.jumpTo({center:[lng,lat],zoom,bearing,pitch})}}
 return <div className="live-presence"><button type="button" aria-pressed={enabled} onClick={()=>{setEnabled(v=>!v);setStatus("")}} title="Chia sẻ con trỏ và vùng nhìn bản đồ với thành viên dự án">● Live {enabled?peers.length:""}</button>{!enabled&&status&&<small>{status}</small>}{enabled&&<div className="live-presence-list"><small>{status} · {peers.length} phiên</small>{peers.filter(p=>p.id!==self).map(p=><button type="button" key={p.id} disabled={blocked||!p.view} aria-pressed={follow===p.id} onClick={()=>choose(p)}><span style={{color:p.color}}>●</span> {p.name} {follow===p.id?"· Đang theo dõi":"· Theo dõi"}</button>)}<small>Chia sẻ vị trí con trỏ, không chia sẻ GPS.</small></div>}</div>
}
