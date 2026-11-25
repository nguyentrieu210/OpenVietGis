import React,{useEffect,useRef,useState,type RefObject} from "react";
import {createPortal} from "react-dom";
import {ApiClient,loadCsrfToken} from "@openvietgis/client-sdk";
import * as ml from "./map-surface";
import type {Peer} from "./LivePresence";

// Spec 058: chat on the project like cursor chat — press "/" to type, the message floats next to your
// cursor for everyone in the project (stored in the project chat, so the log keeps it).
const REACT=/^⟦react:([\w-]{1,64}):(.{1,16})⟧$/u;
export const REACTIONS=["❤️","👍","😆","😮","😢","🔥"];
const onlyEmoji=(t:string)=>/^(\p{Extended_Pictographic}|\uFE0F|\u200D|\s)+$/u.test(t);
type Msg={id:string;author_id:string;author_name:string;body:string;created_at:string};
type Props={client:ApiClient;org:string;pid:string;map:RefObject<ml.Map|null>;peers:Peer[];accountId:string;canWrite:boolean};
export function MapChat({client,org,pid,map,peers,accountId,canWrite}:Props){
 const base="/organizations/"+org+"/projects/"+pid+"/collaboration";
 const [log,setLog]=useState<Msg[]>([]),[typing,setTyping]=useState<{x:number;y:number}|null>(null),[text,setText]=useState(""),[open,setOpen]=useState(false),[unread,setUnread]=useState(0),[bubbles,setBubbles]=useState<Msg[]>([]),[bursts,setBursts]=useState<{id:string;e:string;x:number;d:number;t:number;r:number}[]>([]);
 const seen=useRef(new Set<string>()),first=useRef(true),mouse=useRef({x:0,y:0}),peersRef=useRef(peers);peersRef.current=peers;const openRef=useRef(false);
 const load=async()=>{try{const r=await client.request<Msg[]|{items:Msg[]}>(base+"/messages?limit=30",{cache:"no-store"});const items=(Array.isArray(r)?r:r.items).slice().sort((a,b)=>a.created_at.localeCompare(b.created_at)||a.id.localeCompare(b.id));
  const fresh=items.filter(m=>!seen.current.has(m.id)).map(m=>{const r=REACT.exec(m.body);return r?{...m,body:r[2]}:m});items.forEach(m=>seen.current.add(m.id));setLog(items);
  if(!first.current&&fresh.length){const isEmoji=(t:string)=>/^(\p{Extended_Pictographic}|\uFE0F|\u200D|\s)+$/u.test(t);
  setBubbles(b=>[...b,...fresh.filter(m=>!isEmoji(m.body))].slice(-8));
  const burst=fresh.filter(m=>isEmoji(m.body)).flatMap(m=>Array.from({length:14},(_,k)=>({id:m.id+":"+k,e:m.body.trim(),x:Math.round(Math.random()*90-45),d:+(k*0.09+Math.random()*0.2).toFixed(2),t:+(2.2+Math.random()*1.2).toFixed(2),r:Math.round(Math.random()*40-20)})));
  if(burst.length){setBursts(b=>[...b,...burst].slice(-120));setTimeout(()=>setBursts(b=>b.filter(x=>!burst.includes(x))),4200)}if(!openRef.current)setUnread(n=>n+fresh.filter(m=>m.author_id!==accountId).length);for(const m of fresh)setTimeout(()=>setBubbles(b=>b.filter(x=>x.id!==m.id)),9000)}first.current=false}catch{/* chat unavailable: keep map usable */}};
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
 const shown=log.filter(m=>!REACT.test(m.body));
 const reactions=new Map<string,Map<string,string[]>>();for(const m of log){const r=REACT.exec(m.body);if(!r)continue;const per=reactions.get(r[1])??new Map<string,string[]>();const who=per.get(r[2])??[];if(!who.includes(m.author_name))who.push(m.author_name);per.set(r[2],who);reactions.set(r[1],per)}
 const submit=(t:string)=>{if(!t.trim())return;setDraft("");setAtBottom(true);void send(t)};
 // Live-stream style overlay on the map: always visible, newest at the bottom, old lines fade out at the top.
 const [pos,setPos]=useState<{x:number;y:number}|null>(()=>{try{const v=JSON.parse(localStorage.getItem("ovg:chat-pos")??"null");return v&&Number.isFinite(v.x)&&Number.isFinite(v.y)?v:null}catch{return null}});
 const dragged=useRef(false);
 const drag=(e:React.PointerEvent<HTMLElement>)=>{if(e.button!==0)return;const panelEl=(e.currentTarget as HTMLElement).closest("aside")!,hostEl=panelEl.parentElement!;const pr=panelEl.getBoundingClientRect(),hr=hostEl.getBoundingClientRect(),dx=e.clientX-pr.left,dy=e.clientY-pr.top,sx=e.clientX,sy=e.clientY;dragged.current=false;
  const mv=(ev:PointerEvent)=>{if(!dragged.current&&Math.abs(ev.clientX-sx)+Math.abs(ev.clientY-sy)<5)return;dragged.current=true;const x=Math.max(0,Math.min(hr.width-pr.width,ev.clientX-hr.left-dx)),y=Math.max(0,Math.min(hr.height-40,ev.clientY-hr.top-dy));setPos({x,y})};
  const up=()=>{removeEventListener("pointermove",mv);removeEventListener("pointerup",up);if(dragged.current)setPos(p=>{try{localStorage.setItem("ovg:chat-pos",JSON.stringify(p))}catch{/* storage unavailable */}return p})};
  addEventListener("pointermove",mv);addEventListener("pointerup",up)};
 const sized=useRef<HTMLElement|null>(null);
 useEffect(()=>{const el=sized.current;if(!el||!open)return;try{const v=JSON.parse(localStorage.getItem("ovg:chat-size")??"null");if(v?.w&&v?.h){el.style.width=v.w+"px";el.style.height=v.h+"px"}}catch{/* storage unavailable */}
  let t:ReturnType<typeof setTimeout>|undefined;const ro=new ResizeObserver(()=>{clearTimeout(t);t=setTimeout(()=>{try{if(el.style.width)localStorage.setItem("ovg:chat-size",JSON.stringify({w:el.offsetWidth,h:el.offsetHeight}))}catch{/* storage unavailable */}},300)});ro.observe(el);return()=>{ro.disconnect();clearTimeout(t)}},[open,host]);
 const panel=<aside ref={sized} className={"ovg-live-chat"+(open?" open":" closed")+(pos?" moved":"")} style={pos?{left:pos.x,top:pos.y,right:"auto"}:undefined} aria-label="Chat dự án">
  <header onPointerDown={drag} title="Kéo để di chuyển · nhấp đúp để về góc phải" onDoubleClick={()=>{setPos(null);try{localStorage.removeItem("ovg:chat-pos")}catch{/* storage unavailable */}}}><button type="button" className="ovg-live-chat-title" onClick={()=>{if(dragged.current){dragged.current=false;return}toggle(!open)}} aria-expanded={open}><span className="dot"/>Chat trực tiếp{viewers>1?<small>· {viewers} người đang xem</small>:null}{!open&&unread?<b className="ovg-chat-badge static">{unread}</b>:null}</button><button type="button" className="ovg-live-chat-min" aria-label={open?"Thu nhỏ chat":"Mở chat"} onClick={()=>toggle(!open)}>{open?"–":"▴"}</button></header>
  {open&&<><div className="ovg-live-chat-list" ref={listRef} onScroll={e=>{const el=e.currentTarget;setAtBottom(el.scrollHeight-el.scrollTop-el.clientHeight<24)}} aria-live="polite">{shown.length?shown.map(m=>{const mine=m.author_id===accountId,rs=reactions.get(m.id);return <div key={m.id} className={"ovg-live-msg"+(mine?" mine":"")+(onlyEmoji(m.body)?" only-emoji":"")}>{!mine&&<span className="av" style={{background:color(m.author_id)}}>{initials(m.author_name)}</span>}<div className="ovg-live-bubble"><p>{!mine&&<b style={{color:color(m.author_id)}}>{m.author_name}</b>}{m.body}<time>{time(m.created_at)}</time></p>{rs&&<span className="ovg-live-reacts" title={[...rs].map(([e,who])=>e+" "+who.join(", ")).join(" · ")}>{[...rs].map(([e,who])=><span key={e}>{e}{who.length>1?who.length:""}</span>)}</span>}{canWrite&&<span className="ovg-live-reactbar" role="group" aria-label="Thả cảm xúc">{REACTIONS.map(e=><button type="button" key={e} aria-label={"Thả "+e} onClick={()=>void send("⟦react:"+m.id+":"+e+"⟧")}>{e}</button>)}</span>}</div></div>}):<p className="ovg-live-empty">Chưa có tin nhắn. Gõ bên dưới hoặc bấm <kbd>/</kbd> trên bản đồ để nhắn cạnh con trỏ.</p>}</div>
  {!atBottom&&<button type="button" className="ovg-live-jump" onClick={()=>{listRef.current?.scrollTo({top:listRef.current.scrollHeight,behavior:"smooth"});setAtBottom(true)}}>↓ Tin mới</button>}
  {canWrite?<form className="ovg-live-input" onSubmit={e=>{e.preventDefault();submit(draft)}}><div className="ovg-live-quick">{["👍","✅","❓","👀","🔥"].map(q=><button type="button" key={q} onClick={()=>submit(q)} aria-label={"Gửi "+q}>{q}</button>)}</div><input value={draft} maxLength={500} placeholder="Nhắn cho cả dự án…" onChange={e=>setDraft(e.target.value)}/><button type="submit" className="send" disabled={!draft.trim()} aria-label="Gửi">➤</button></form>:<p className="ovg-live-empty">Bạn chỉ có quyền xem.</p>}</>}
 </aside>;
 return <>
  <button type="button" className="plugin-tool ovg-chat" aria-pressed={open} title="Chat trực tiếp · phím / để nhắn cạnh con trỏ" onClick={()=>toggle(!open)}>💬{unread&&!open?<b className="ovg-chat-badge">{unread}</b>:null}</button>
  {host&&createPortal(panel,host)}
  {typing&&<form className="ovg-cursor-chat" style={{left:typing.x+14,top:typing.y+14}} onSubmit={e=>{e.preventDefault();const t=text;setTyping(null);setText("");void send(t)}}><input autoFocus value={text} maxLength={300} placeholder="Nhắn… (Enter gửi, Esc hủy)" onChange={e=>setText(e.target.value)} onKeyDown={e=>{if(e.key==="Escape"){e.preventDefault();setTyping(null)}}} onBlur={()=>setTyping(null)}/></form>}
  {host&&createPortal(<div className="ovg-danmaku" aria-hidden="true">{bubbles.map((m,i)=><div key={m.id} className="ovg-danmaku-line" style={{top:8+(i%3)*34,"--peer-color":color(m.author_id)} as React.CSSProperties}><span className="av" style={{background:color(m.author_id)}}>{initials(m.author_name)}</span><b>{m.author_id===accountId?"Bạn":m.author_name}</b>{m.body.length>140?m.body.slice(0,140)+"…":m.body}</div>)}</div>,host)}
  {host&&bursts.length>0&&createPortal(<div className="ovg-burst" aria-hidden="true">{bursts.map(b=><span key={b.id} style={{"--x":b.x+"px","--r":b.r+"deg",animationDelay:b.d+"s",animationDuration:b.t+"s"} as React.CSSProperties}>{b.e}</span>)}</div>,host)}
 </>;
}
