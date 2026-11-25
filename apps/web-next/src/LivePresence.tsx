import React,{useEffect,useRef,useState,type RefObject} from "react";
import * as ml from "./map-surface";
import "./live-presence.css";
export type Peer={id:string;actor:string;name:string;color:string;cursor:number[]|null;view:number[]|null};
type Props={org:string;pid:string;map:RefObject<ml.Map|null>;blocked:boolean;preserveView:()=>void;onPeers?:(peers:Peer[],self:string)=>void};
// Spec 058: on by default like Google Sheets — avatars of who is in the project, coloured cursors, follow a person.
export function LivePresence({org,pid,map,blocked,preserveView,onPeers}:Props){
 const onPeersRef=useRef(onPeers);onPeersRef.current=onPeers;
 const [enabled,setEnabled]=useState(true),[status,setStatus]=useState(""),[peers,setPeers]=useState<Peer[]>([]),[self,setSelf]=useState(""),[follow,setFollow]=useState("");
 const followRef=useRef(follow),blockedRef=useRef(blocked),preserve=useRef(preserveView);followRef.current=follow;blockedRef.current=blocked;preserve.current=preserveView;
 useEffect(()=>{if(blocked)setFollow("")},[blocked]);
 useEffect(()=>{
  if(!enabled||!map.current)return;
  const m=map.current,markers=new Map<string,ml.Marker>();let stopped=false,ws:WebSocket|null=null,retry:ReturnType<typeof setTimeout>|undefined,timer:ReturnType<typeof setTimeout>|undefined,delay=1000,last=0,pending:Record<string,unknown>={},me="";
  const clear=()=>{markers.forEach(x=>x.remove());markers.clear();setPeers([]);setSelf("");setFollow("");onPeersRef.current?.([],"")};
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
    const users:Peer[]=packet.users;me=packet.self_id;setSelf(me);setPeers(users);onPeersRef.current?.(users,me);
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
 const me=peers.find(p=>p.id===self),others=peers.filter(p=>p.id!==self&&p.actor!==me?.actor),initials=(n:string)=>n.trim().split(/s+/).slice(-2).map(w=>w[0]??"").join("").toUpperCase()||"?";
 return <div className="live-presence ovg-avatars" aria-label="Người đang ở trong dự án">{enabled&&others.slice(0,6).map(p=><button type="button" key={p.id} className="ovg-avatar" style={{"--peer-color":p.color} as React.CSSProperties} aria-pressed={follow===p.id} disabled={blocked||!p.view} title={p.name+(follow===p.id?" · đang theo dõi vùng nhìn (bấm để thôi)":" · bấm để tới chỗ người này đang xem")} onClick={()=>choose(p)}>{initials(p.name)}</button>)}{enabled&&others.length>6&&<span className="ovg-avatar more" title={others.slice(6).map(p=>p.name).join(", ")}>+{others.length-6}</span>}<button type="button" className="ovg-presence-toggle" aria-pressed={enabled} onClick={()=>{setEnabled(v=>!v);setStatus("")}} title={enabled?(status||"Trực tuyến")+" · "+(others.length?others.length+" người khác đang mở dự án":"chỉ có bạn")+" · bấm để tắt chia sẻ con trỏ":"Đang tắt chia sẻ · bấm để bật"}>{enabled?(others.length?"●":"○"):"⊘"}</button></div>
}
