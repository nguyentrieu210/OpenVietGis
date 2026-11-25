import React,{useEffect,useRef,useState,type RefObject} from "react";
import {createPortal} from "react-dom";
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
 const seen=useRef(new Set<string>()),first=useRef(true),mouse=useRef({x:0,y:0}),peersRef=useRef(peers);peersRef.current=peers;const openRef=useRef(false);
 const load=async()=>{try{const r=await client.request<Msg[]|{items:Msg[]}>(base+"/messages?limit=30",{cache:"no-store"});const items=(Array.isArray(r)?r:r.items).slice().reverse();
  const fresh=items.filter(m=>!seen.current.has(m.id));items.forEach(m=>seen.current.add(m.id));setLog(items);
  if(!first.current&&fresh.length){setBubbles(b=>[...b,...fresh].slice(-8));if(!openRef.current)setUnread(n=>n+fresh.filter(m=>m.author_id!==accountId).length);for(const m of fresh)setTimeout(()=>setBubbles(b=>b.filter(x=>x.id!==m.id)),9000)}first.current=false}catch{/* chat unavailable: keep map usable */}};
 useEffect(()=>{first.current=true;seen.current.clear();void load();const es=new EventSource("/api/v1"+base+"/events",{withCredentials:true});const on=()=>void load();es.addEventListener("change",on);es.addEventListener("ready",on);
  const poll=setInterval(()=>void load(),30000);return()=>{es.close();clearInterval(poll)}},[org,pid]);
 useEffect(()=>{const mv=(e:MouseEvent)=>{mouse.current={x:e.clientX,y:e.clientY}};const start=()=>{if(!canWrite)return;const {x,y}=mouse.current;setTyping({x:x||innerWidth/2,y:y||innerHeight/2});setText("")};
  window.addEventListener("mousemove",mv);window.addEventListener("ovg:chat",start);return()=>{window.removeEventListener("mousemove",mv);window.removeEventListener("ovg:chat",start)}},[canWrite]);
 const send=async(body:string)=>{const t=body.trim().slice(0,4000);if(!t)return;await loadCsrfToken(client);await client.request(base+"/messages",{method:"POST",body:JSON.stringify({body:t})});void load()};
 const color=(actor:string)=>peers.find(p=>p.actor===actor)?.color??"#1565a9";
 const time=(s:string)=>new Date(s).toLocaleTimeString("vi-VN",{hour:"2-digit",minute:"2-digit"});
 openRef.current=open;
 const host=map.current?.getContainer().parentElement??null,listRef=useRef<HTMLDivElement>(null),[atBottom,setAtBottom]=useState(true),[draft,setDraft]=useState("");
 useEffect(()=>{if(open&&atBottom)listRef.current?.scrollTo({top:listRef.current.scrollHeight})},[log,open]);
 useEffect(()=>{try{setOpen(localStorage.getItem("ovg:chat-open")!=="0")}catch{/* storage unavailable */}},[]);
 const toggle=(v:boolean)=>{setOpen(v);if(v)setUnread(0);try{localStorage.setItem("ovg:chat-open",v?"1":"0")}catch{/* storage unavailable */}};
 const initials=(n:string)=>n.trim().split(/\s+/).slice(-2).map(w=>w[0]??"").join("").toUpperCase()||"?";
 const viewers=new Set(peers.map(p=>p.actor)).size;
 const submit=(t:string)=>{if(!t.trim())return;setDraft("");setAtBottom(true);void send(t)};
 // Live-stream style overlay on the map: always visible, newest at the bottom, old lines fade out at the top.
 const panel=<aside className={"ovg-live-chat"+(open?" open":" closed")} aria-label="Chat dự án">
  <header><button type="button" className="ovg-live-chat-title" onClick={()=>toggle(!open)} aria-expanded={open}><span className="dot"/>Chat trực tiếp{viewers>1?<small>· {viewers} người đang xem</small>:null}{!open&&unread?<b className="ovg-chat-badge static">{unread}</b>:null}</button><button type="button" className="ovg-live-chat-min" aria-label={open?"Thu nhỏ chat":"Mở chat"} onClick={()=>toggle(!open)}>{open?"–":"▴"}</button></header>
  {open&&<><div className="ovg-live-chat-list" ref={listRef} onScroll={e=>{const el=e.currentTarget;setAtBottom(el.scrollHeight-el.scrollTop-el.clientHeight<24)}} aria-live="polite">{log.length?log.map(m=><div key={m.id} className={"ovg-live-msg"+(m.author_id===accountId?" mine":"")}><span className="av" style={{background:color(m.author_id)}}>{initials(m.author_name)}</span><p><b style={{color:color(m.author_id)}}>{m.author_id===accountId?"Bạn":m.author_name}</b><time>{time(m.created_at)}</time><br/>{m.body}</p></div>):<p className="ovg-live-empty">Chưa có tin nhắn. Gõ bên dưới hoặc bấm <kbd>/</kbd> trên bản đồ để nhắn cạnh con trỏ.</p>}</div>
  {!atBottom&&<button type="button" className="ovg-live-jump" onClick={()=>{listRef.current?.scrollTo({top:listRef.current.scrollHeight,behavior:"smooth"});setAtBottom(true)}}>↓ Tin mới</button>}
  {canWrite?<form className="ovg-live-input" onSubmit={e=>{e.preventDefault();submit(draft)}}><div className="ovg-live-quick">{["👍","✅","❓","👀","🔥"].map(q=><button type="button" key={q} onClick={()=>submit(q)} aria-label={"Gửi "+q}>{q}</button>)}</div><input value={draft} maxLength={500} placeholder="Nhắn cho cả dự án…" onChange={e=>setDraft(e.target.value)}/><button type="submit" className="send" disabled={!draft.trim()} aria-label="Gửi">➤</button></form>:<p className="ovg-live-empty">Bạn chỉ có quyền xem.</p>}</>}
 </aside>;
 return <>
  <button type="button" className="plugin-tool ovg-chat" aria-pressed={open} title="Chat trực tiếp · phím / để nhắn cạnh con trỏ" onClick={()=>toggle(!open)}>💬{unread&&!open?<b className="ovg-chat-badge">{unread}</b>:null}</button>
  {host&&createPortal(panel,host)}
  {typing&&<form className="ovg-cursor-chat" style={{left:typing.x+14,top:typing.y+14}} onSubmit={e=>{e.preventDefault();const t=text;setTyping(null);setText("");void send(t)}}><input autoFocus value={text} maxLength={300} placeholder="Nhắn… (Enter gửi, Esc hủy)" onChange={e=>setText(e.target.value)} onKeyDown={e=>{if(e.key==="Escape"){e.preventDefault();setTyping(null)}}} onBlur={()=>setTyping(null)}/></form>}
  {host&&createPortal(<div className="ovg-danmaku" aria-hidden="true">{bubbles.map((m,i)=><div key={m.id} className="ovg-danmaku-line" style={{top:8+(i%3)*30,"--peer-color":color(m.author_id)} as React.CSSProperties}><b>{m.author_id===accountId?"Bạn":m.author_name}</b>{m.body.length>140?m.body.slice(0,140)+"…":m.body}</div>)}</div>,host)}
 </>;
}
