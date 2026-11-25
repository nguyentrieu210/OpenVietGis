import React,{useEffect,useRef,useState,type RefObject} from "react";
import {createPortal} from "react-dom";
import {ApiClient,loadCsrfToken} from "@openvietgis/client-sdk";
import * as ml from "./map-surface";
import type {Peer} from "./LivePresence";

// Spec 058: app-level chat rail on the right edge (like Zalo PC): the project room plus 1–1 messages with
// project members. The project room also shows on the map: running lines, emoji bursts, "/" cursor chat.
// Reactions are tiny messages "⟦react:<id>:<emoji>⟧" (no server change). shortcut: group rooms and push
// delivery need server work (separate spec).
const REACT=/^⟦react:([\w-]{1,64}):(.{1,16})⟧$/u;
export const REACTIONS=["❤️","👍","😆","😮","😢","🔥"];
const onlyEmoji=(t:string)=>/^(\p{Extended_Pictographic}|️|‍|\s)+$/u.test(t);
const initials=(n:string)=>n.trim().split(/\s+/).slice(-2).map(w=>w[0]??"").join("").toUpperCase()||"?";
const hue=(id:string)=>"hsl("+[...id].reduce((h,c)=>(h*31+c.charCodeAt(0))%360,0)+" 55% 45%)";
type Msg={id:string;author_id:string;author_name:string;body:string;created_at:string;recipient_id?:string|null};
type Member={id:string;display_name:string};
type Burst={id:string;e:string;x:number;d:number;t:number;r:number};
type Props={client:ApiClient;org:string;pid:string;map:RefObject<ml.Map|null>;peers:Peer[];accountId:string;canWrite:boolean};
const sortMsgs=(l:Msg[])=>l.slice().sort((a,b)=>a.created_at.localeCompare(b.created_at)||a.id.localeCompare(b.id));
const makeBurst=(id:string,e:string):Burst[]=>Array.from({length:14},(_,k)=>({id:id+":"+k,e,x:Math.round(Math.random()*90-45),d:+(k*0.09+Math.random()*0.2).toFixed(2),t:+(2.2+Math.random()*1.2).toFixed(2),r:Math.round(Math.random()*40-20)}));

export function MapChat({client,org,pid,map,peers,accountId,canWrite}:Props){
 const base="/organizations/"+org+"/projects/"+pid+"/collaboration";
 const [people,setPeople]=useState(false),[who,setWho]=useState(""),[room,setRoom]=useState(""),[open,setOpen]=useState(()=>{try{return localStorage.getItem("ovg:chat-open")!=="0"}catch{return true}});
 const [logs,setLogs]=useState<Record<string,Msg[]>>({}),[members,setMembers]=useState<Member[]>([]),[unread,setUnread]=useState<Record<string,number>>({});
 const [bubbles,setBubbles]=useState<Msg[]>([]),[bursts,setBursts]=useState<Burst[]>([]),[typing,setTyping]=useState<{x:number;y:number}|null>(null),[text,setText]=useState(""),[draft,setDraft]=useState("");
 const seen=useRef(new Set<string>()),primed=useRef(new Set<string>()),mouse=useRef({x:0,y:0}),view=useRef({room:"",open});view.current={room,open};
 const listRef=useRef<HTMLDivElement>(null);
 const announce=(fresh:Msg[])=>{const text=fresh.filter(m=>!onlyEmoji((REACT.exec(m.body)?.[2])??m.body)&&!REACT.test(m.body));setBubbles(b=>[...b,...text].slice(-8));for(const m of text)setTimeout(()=>setBubbles(b=>b.filter(x=>x.id!==m.id)),9000);
  const burst=fresh.flatMap(m=>{const e=(REACT.exec(m.body)?.[2])??m.body;return onlyEmoji(e)?makeBurst(m.id,e.trim()):[]});if(burst.length){setBursts(b=>[...b,...burst].slice(-120));setTimeout(()=>setBursts(b=>b.filter(x=>!burst.includes(x))),4200)}};
 const load=async(r:string,limit=40)=>{try{const res=await client.request<{items:Msg[]}|Msg[]>(base+"/messages?limit="+limit+(r?"&peer="+encodeURIComponent(r):""),{cache:"no-store"});const items=sortMsgs(Array.isArray(res)?res:res.items);
  const fresh=items.filter(m=>!seen.current.has(m.id));items.forEach(m=>seen.current.add(m.id));setLogs(l=>({...l,[r]:items}));
  if(primed.current.has(r)&&fresh.length){const others=fresh.filter(m=>m.author_id!==accountId);if(!r)announce(others);if(others.length&&!(view.current.open&&view.current.room===r))setUnread(u=>({...u,[r]:(u[r]??0)+others.length}))}primed.current.add(r)}catch{/* chat unavailable: keep the map usable */}};
 useEffect(()=>{seen.current.clear();primed.current.clear();setLogs({});setUnread({});void load("");
  client.request<{items:Member[]}>(base+"/members",{cache:"no-store"}).then(r=>{setMembers(r.items.filter(m=>m.id!==accountId));r.items.filter(m=>m.id!==accountId).slice(0,20).forEach(m=>void load(m.id,20))}).catch(()=>setMembers([]));
  const es=new EventSource("/api/v1"+base+"/events",{withCredentials:true});const on=()=>{void load("");if(view.current.room)void load(view.current.room)};es.addEventListener("change",on);
  const poll=setInterval(()=>{void load("");setMembers(ms=>{ms.slice(0,20).forEach(m=>void load(m.id,20));return ms})},30000);return()=>{es.close();clearInterval(poll)}},[org,pid]);
 useEffect(()=>{const mv=(e:MouseEvent)=>{mouse.current={x:e.clientX,y:e.clientY}};const start=()=>{if(!canWrite)return;const {x,y}=mouse.current;setTyping({x:x||innerWidth/2,y:y||innerHeight/2});setText("")};
  window.addEventListener("mousemove",mv);window.addEventListener("ovg:chat",start);return()=>{window.removeEventListener("mousemove",mv);window.removeEventListener("ovg:chat",start)}},[canWrite]);
 const send=async(body:string,to=room)=>{const t=body.trim().slice(0,4000);if(!t)return;
  const temp:Msg={id:"local-"+crypto.randomUUID(),author_id:accountId,author_name:peers.find(p=>p.actor===accountId)?.name??"Bạn",body:t,created_at:new Date().toISOString(),recipient_id:to||null};
  setLogs(l=>({...l,[to]:[...(l[to]??[]),temp]}));if(!to)announce([temp]);
  try{await loadCsrfToken(client);const saved=await client.request<Msg>(base+"/messages",{method:"POST",body:JSON.stringify(to?{body:t,recipient_id:to}:{body:t})});if(saved?.id)seen.current.add(saved.id)}
  catch{setLogs(l=>({...l,[to]:(l[to]??[]).filter(m=>m.id!==temp.id)}));return}
  void load(to)};
 const pick=(r:string)=>{if(open&&room===r){setOpen(false);try{localStorage.setItem("ovg:chat-open","0")}catch{/* storage unavailable */}return}setRoom(r);setOpen(true);setUnread(u=>({...u,[r]:0}));try{localStorage.setItem("ovg:chat-open","1")}catch{/* storage unavailable */}void load(r)};
 useEffect(()=>{listRef.current?.scrollTo({top:listRef.current.scrollHeight})},[logs,room,open]);
 const color=(actor:string)=>peers.find(p=>p.actor===actor)?.color??hue(actor);
 const time=(s:string)=>new Date(s).toLocaleTimeString("vi-VN",{hour:"2-digit",minute:"2-digit"});
 const online=new Set(peers.map(p=>p.actor));
 const log=logs[room]??[],shown=log.filter(m=>!REACT.test(m.body));
 const reactions=new Map<string,Map<string,string[]>>();for(const m of log){const r=REACT.exec(m.body);if(!r)continue;const per=reactions.get(r[1])??new Map<string,string[]>();const who=per.get(r[2])??[];if(!who.includes(m.author_name))who.push(m.author_name);per.set(r[2],who);reactions.set(r[1],per)}
 const roomName=room?members.find(m=>m.id===room)?.display_name??"Nhắn riêng":"Phòng dự án";
 const submit=(t:string)=>{if(!t.trim())return;setDraft("");void send(t)};
 const host=map.current?.getContainer().parentElement??null;
 const total=Object.values(unread).reduce((a,b)=>a+b,0);
 const lastOf=(r:string)=>{const l=(logs[r]??[]).filter(m=>!REACT.test(m.body));return l[l.length-1]};
 const rail=<aside className={"ovg-chat-rail"+(open?" open":"")} aria-label="Chat">
  <nav className="ovg-rail" aria-label="Phòng chat">
   <button type="button" className={"ovg-rail-room project"+(open&&!room?" active":"")} title="Phòng dự án · mọi thành viên" onClick={()=>pick("")}>#{unread[""]?<b className="ovg-chat-badge">{unread[""]}</b>:null}</button>
   <button type="button" className={"ovg-rail-room add"+(people?" active":"")} title="Nhắn tin mới · tìm người, tạo nhóm" onClick={()=>{setPeople(v=>!v);setWho("")}}>＋</button>
   <span className="ovg-rail-sep"/>
   {members.slice(0,20).map(m=><button type="button" key={m.id} className={"ovg-rail-room"+(open&&room===m.id?" active":"")} style={{background:color(m.id)}} title={m.display_name+(online.has(m.id)?" · đang trực tuyến":"")+" · nhắn riêng"} onClick={()=>pick(m.id)}>{initials(m.display_name)}{online.has(m.id)&&<i className="online"/>}{unread[m.id]?<b className="ovg-chat-badge">{unread[m.id]}</b>:null}</button>)}
  </nav>
  {people&&<section className="ovg-chat-pane ovg-people">
   <header><b>Nhắn tin mới</b><small>Người trong dự án có quyền sửa</small><button type="button" aria-label="Đóng" onClick={()=>setPeople(false)}>×</button></header>
   <div className="ovg-people-search"><input autoFocus value={who} onChange={e=>setWho(e.target.value)} placeholder="Tìm theo tên…" aria-label="Tìm người"/></div>
   <div className="ovg-chat-msgs">{members.filter(m=>m.display_name.toLocaleLowerCase("vi").includes(who.toLocaleLowerCase("vi"))).map(m=><button type="button" key={m.id} className="ovg-person" onClick={()=>{setPeople(false);pick(m.id)}}><span className="av" style={{background:color(m.id)}}>{initials(m.display_name)}</span><span><b>{m.display_name}</b><small>{online.has(m.id)?"Đang trực tuyến":"Nhắn riêng"}</small></span></button>)}
    {!members.length&&<p className="ovg-live-empty">Dự án chưa có ai khác. Mời người vào tổ chức và cấp quyền sửa dự án (Phân quyền) để nhắn riêng.</p>}
    <div className="ovg-people-soon"><b>＋ Tạo nhóm chat</b><b>🔍 Tìm người ngoài dự án · kết bạn</b><small>Đang làm ở bước 3 (cần máy chủ): nhóm riêng nhiều người, phòng theo tổ chức, danh bạ và lời mời kết bạn.</small></div>
   </div></section>}
  {open&&!people&&<section className="ovg-chat-pane">
   <header><b>{room?"":"# "}{roomName}</b><small>{room?(online.has(room)?"Đang trực tuyến":"Nhắn riêng · chỉ hai người thấy"):members.length+1+" thành viên · tin cũng chạy trên bản đồ"}</small><button type="button" aria-label="Thu gọn chat" onClick={()=>pick(room)}>×</button></header>
   <div className="ovg-chat-msgs" ref={listRef} aria-live="polite">{shown.length?shown.map(m=>{const mine=m.author_id===accountId,rs=reactions.get(m.id);return <div key={m.id} className={"ovg-live-msg"+(mine?" mine":"")+(onlyEmoji(m.body)?" only-emoji":"")}>{!mine&&<span className="av" style={{background:color(m.author_id)}}>{initials(m.author_name)}</span>}<div className="ovg-live-bubble"><p>{!mine&&!room&&<b style={{color:color(m.author_id)}}>{m.author_name}</b>}{m.body}<time>{time(m.created_at)}</time></p>{rs&&<span className="ovg-live-reacts" title={[...rs].map(([e,who])=>e+" "+who.join(", ")).join(" · ")}>{[...rs].map(([e,who])=><span key={e}>{e}{who.length>1?who.length:""}</span>)}</span>}{canWrite&&<span className="ovg-live-reactbar" role="group" aria-label="Thả cảm xúc">{REACTIONS.map(e=><button type="button" key={e} aria-label={"Thả "+e} onClick={()=>void send("⟦react:"+m.id+":"+e+"⟧")}>{e}</button>)}</span>}</div></div>}):<p className="ovg-live-empty">{room?"Bắt đầu cuộc trò chuyện với "+roomName+".":"Chưa có tin nhắn. Bấm / trên bản đồ để nhắn cạnh con trỏ."}</p>}</div>
   {canWrite?<form className="ovg-live-input" onSubmit={e=>{e.preventDefault();submit(draft)}}><div className="ovg-live-quick">{["👍","✅","❓","👀","🔥"].map(q=><button type="button" key={q} onClick={()=>submit(q)} aria-label={"Gửi "+q}>{q}</button>)}</div><input value={draft} maxLength={4000} placeholder={room?"Nhắn riêng cho "+roomName+"…":"Nhắn cho cả dự án…"} onChange={e=>setDraft(e.target.value)}/><button type="submit" className="send" disabled={!draft.trim()} aria-label="Gửi">➤</button></form>:<p className="ovg-live-empty">Bạn chỉ có quyền xem.</p>}
  </section>}
 </aside>;
 void lastOf;
 return <>
  <button type="button" className="plugin-tool ovg-chat" aria-pressed={open} title="Chat · phím / để nhắn cạnh con trỏ" onClick={()=>pick(room)}>💬{total&&!open?<b className="ovg-chat-badge">{total}</b>:null}</button>
  {createPortal(<div className="qgis-studio ovg-chat-root">{rail}</div>,document.body)}
  {typing&&<form className="ovg-cursor-chat" style={{left:typing.x+14,top:typing.y+14}} onSubmit={e=>{e.preventDefault();const t=text;setTyping(null);setText("");void send(t,"")}}><input autoFocus value={text} maxLength={300} placeholder="Nhắn cả dự án… (Enter gửi, Esc hủy)" onChange={e=>setText(e.target.value)} onKeyDown={e=>{if(e.key==="Escape"){e.preventDefault();setTyping(null)}}} onBlur={()=>setTyping(null)}/></form>}
  {host&&bubbles.length>0&&createPortal(<div className="ovg-danmaku" aria-hidden="true" style={(r=>({left:r.left,top:r.top+4,width:r.width}))(host.getBoundingClientRect())}>{bubbles.map((m,i)=><div key={m.id} className="ovg-danmaku-line" style={{top:8+(i%3)*34,"--peer-color":color(m.author_id)} as React.CSSProperties}><span className="av" style={{background:color(m.author_id)}}>{initials(m.author_name)}</span><b>{m.author_id===accountId?"Bạn":m.author_name}</b>{m.body.length>140?m.body.slice(0,140)+"…":m.body}</div>)}</div>,document.body)}
  {host&&bursts.length>0&&createPortal(<div className="ovg-burst" aria-hidden="true" style={(r=>({left:r.right-90,top:r.bottom-70}))(host.getBoundingClientRect())}>{bursts.map(b=><span key={b.id} style={{"--x":b.x+"px","--r":b.r+"deg",animationDelay:b.d+"s",animationDuration:b.t+"s"} as React.CSSProperties}>{b.e}</span>)}</div>,document.body)}
 </>;
}
