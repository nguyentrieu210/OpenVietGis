import React,{useEffect,useRef,useState,type RefObject} from "react";
import {ApiClient,loadCsrfToken} from "@openvietgis/client-sdk";
import * as ml from "./map-surface";
import type {Peer} from "./LivePresence";

// Spec 058: chat on the project like cursor chat — press "/" to type, the message floats next to your
// cursor for everyone in the project (stored in the project chat, so the log keeps it).
type Msg={id:string;author_id:string;author_name:string;body:string;created_at:string};
type Props={client:ApiClient;org:string;pid:string;map:RefObject<ml.Map|null>;peers:Peer[];accountId:string;canWrite:boolean};
export function MapChat({client,org,pid,map,peers,accountId,canWrite}:Props){
 const base="/organizations/"+org+"/projects/"+pid+"/collaboration";
 const [log,setLog]=useState<Msg[]>([]),[typing,setTyping]=useState<{x:number;y:number}|null>(null),[text,setText]=useState(""),[open,setOpen]=useState(false),[unread,setUnread]=useState(0),[bubbles,setBubbles]=useState<Msg[]>([]);
 const seen=useRef(new Set<string>()),first=useRef(true),mouse=useRef({x:0,y:0}),peersRef=useRef(peers);peersRef.current=peers;
 const load=async()=>{try{const r=await client.request<Msg[]|{items:Msg[]}>(base+"/messages?limit=30",{cache:"no-store"});const items=(Array.isArray(r)?r:r.items).slice().reverse();
  const fresh=items.filter(m=>!seen.current.has(m.id));items.forEach(m=>seen.current.add(m.id));setLog(items);
  if(!first.current&&fresh.length){setBubbles(b=>[...b,...fresh].slice(-6));setUnread(n=>n+fresh.filter(m=>m.author_id!==accountId).length);for(const m of fresh)setTimeout(()=>setBubbles(b=>b.filter(x=>x.id!==m.id)),9000)}first.current=false}catch{/* chat unavailable: keep map usable */}};
 useEffect(()=>{first.current=true;seen.current.clear();void load();const es=new EventSource("/api/v1"+base+"/events",{withCredentials:true});const on=()=>void load();es.addEventListener("change",on);es.addEventListener("ready",on);
  const poll=setInterval(()=>void load(),30000);return()=>{es.close();clearInterval(poll)}},[org,pid]);
 useEffect(()=>{const mv=(e:MouseEvent)=>{mouse.current={x:e.clientX,y:e.clientY}};const start=()=>{if(!canWrite)return;const {x,y}=mouse.current;setTyping({x:x||innerWidth/2,y:y||innerHeight/2});setText("")};
  window.addEventListener("mousemove",mv);window.addEventListener("ovg:chat",start);return()=>{window.removeEventListener("mousemove",mv);window.removeEventListener("ovg:chat",start)}},[canWrite]);
 const send=async(body:string)=>{const t=body.trim().slice(0,4000);if(!t)return;await loadCsrfToken(client);await client.request(base+"/messages",{method:"POST",body:JSON.stringify({body:t})});void load()};
 // bubble position: next to the author's live cursor if visible, else stacked at the bottom-left
 const where=(m:Msg,i:number):React.CSSProperties=>{const p=peersRef.current.find(x=>x.actor===m.author_id&&x.cursor);const mp=map.current;if(p?.cursor&&mp&&m.author_id!==accountId){const pt=mp.project(p.cursor as [number,number]);const r=mp.getContainer().getBoundingClientRect();return {left:r.left+pt.x+18,top:r.top+pt.y+30}}return {left:16+(document.querySelector(".qgis-dock")?.getBoundingClientRect().right??0),bottom:96+i*40}};
 const color=(actor:string)=>peers.find(p=>p.actor===actor)?.color??"#1565a9";
 const time=(s:string)=>new Date(s).toLocaleTimeString("vi-VN",{hour:"2-digit",minute:"2-digit"});
 return <>
  <details className="ovg-hotkey-switch ovg-chat" open={open} onToggle={e=>{const o=(e.currentTarget as HTMLDetailsElement).open;setOpen(o);if(o)setUnread(0)}}><summary className="plugin-tool" title="Chat dự án · phím / để gõ nhanh trên bản đồ">💬{unread?<b className="ovg-chat-badge">{unread}</b>:null}</summary>
   <div className="ovg-hotkey-menu ovg-chat-log"><div className="ovg-chat-list">{log.length?log.map(m=><p key={m.id}><b style={{color:color(m.author_id)}}>{m.author_id===accountId?"Bạn":m.author_name}</b> <small>{time(m.created_at)}</small><br/>{m.body}</p>):<small>Chưa có tin nhắn. Bấm / trên bản đồ để gõ nhanh.</small>}</div>
   {canWrite&&<form onSubmit={e=>{e.preventDefault();const t=text;setText("");void send(t)}}><input value={typing?"":text} onChange={e=>setText(e.target.value)} placeholder="Nhắn cho cả dự án…" maxLength={4000}/></form>}</div></details>
  {typing&&<form className="ovg-cursor-chat" style={{left:typing.x+14,top:typing.y+14}} onSubmit={e=>{e.preventDefault();const t=text;setTyping(null);setText("");void send(t)}}><input autoFocus value={text} maxLength={300} placeholder="Nhắn… (Enter gửi, Esc hủy)" onChange={e=>setText(e.target.value)} onKeyDown={e=>{if(e.key==="Escape"){e.preventDefault();setTyping(null)}}} onBlur={()=>setTyping(null)}/></form>}
  {bubbles.map((m,i)=><div key={m.id} className="ovg-chat-bubble" style={{...where(m,i),"--peer-color":color(m.author_id)} as React.CSSProperties}><b>{m.author_id===accountId?"Bạn":m.author_name}</b>{m.body.length>140?m.body.slice(0,140)+"…":m.body}</div>)}
 </>;
}
