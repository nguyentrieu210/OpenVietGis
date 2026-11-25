import {useCallback,useEffect,useRef,useState} from "react";
import {ApiClient,ApiError,loadCsrfToken} from "@openvietgis/client-sdk";
import {EmojiPicker} from "./EmojiPicker";
import "./org-chat.css";

type Person={id:string;display_name:string;contact?:boolean;request?:"incoming"|"outgoing"|null};
type Room={id:string;kind:"org"|"group"|"direct";name:string;member_count:number;unread:number;mentioned:boolean;peer:Person|null;last_message:{body:string;author_name:string}|null};
type Message={id:string;author_id:string;author_name:string;body:string;created_at:string;pinned:boolean;reply_to:{id:string;author_name:string;body:string}|null;mentions:Person[];attachments:{id:string;kind:"image";width:number;height:number}[];reactions:{emoji:string;count:number;mine:boolean}[]};
type Contact={user_id:string;display_name:string;id?:string};
type Contacts={items:Contact[];incoming:Contact[];outgoing:Contact[]};
type ImageDraft={kind:"image";data_url:string;width:number;height:number};
type Items<T>={items:T[]};
const emojis=["❤️","👍","😆","😮","😢","🔥"];
const emptyContacts:Contacts={items:[],incoming:[],outgoing:[]};

export async function prepareChatImage(file:File):Promise<ImageDraft>{
 if(!["image/png","image/jpeg","image/webp"].includes(file.type)||file.size>400_000)throw Error("Ảnh PNG/JPEG/WebP tối đa 400 KB.");
 const data=await new Promise<string>((resolve,reject)=>{const r=new FileReader();r.onload=()=>resolve(String(r.result));r.onerror=()=>reject(Error("Không đọc được ảnh"));r.readAsDataURL(file)});
 const img=new Image();img.src=data;await img.decode();
 if(!img.naturalWidth||!img.naturalHeight||img.naturalWidth>4096||img.naturalHeight>4096)throw Error("Kích thước ảnh tối đa 4096 × 4096.");
 return {kind:"image",data_url:data,width:img.naturalWidth,height:img.naturalHeight};
}

export function OrgChat({client,org,accountId,close}:{client:ApiClient;org:string;accountId:string;close:()=>void}){
 const base="/organizations/"+encodeURIComponent(org)+"/chat";
 const [rooms,setRooms]=useState<Room[]>([]),[roomId,setRoomId]=useState(""),[messages,setMessages]=useState<Message[]>([]),[pins,setPins]=useState<Message[]>([]);
 const [roomSearch,setRoomSearch]=useState("");
 const [tab,setTab]=useState<"rooms"|"people"|"contacts"|"group">("rooms"),[q,setQ]=useState(""),[people,setPeople]=useState<Person[]>([]),[contacts,setContacts]=useState<Contacts>(emptyContacts);
 const [draft,setDraft]=useState(""),[reply,setReply]=useState<Message|null>(null),[mentions,setMentions]=useState<Person[]>([]),[images,setImages]=useState<ImageDraft[]>([]),[emoji,setEmoji]=useState(false),[tag,setTag]=useState(false);
 const [groupName,setGroupName]=useState(""),[groupUsers,setGroupUsers]=useState<Person[]>([]),[groupAction,setGroupAction]=useState<"create"|"invite">("create");
 const [error,setError]=useState(""),[busy,setBusy]=useState(false),[connected,setConnected]=useState(false),[showPins,setShowPins]=useState(false),[more,setMore]=useState(true);
 const accessRevoked=useRef(false),active=useRef(""),requestSeq=useRef(0),rosterSeq=useRef(0),alive=useRef(true),sending=useRef(false),scroller=useRef<HTMLDivElement>(null);
 const selected=rooms.find(r=>r.id===roomId);
 const visibleRooms=rooms.filter(r=>r.name.toLocaleLowerCase("vi").includes(roomSearch.toLocaleLowerCase("vi")));
 const report=useCallback(async(e:unknown)=>{const detail=e instanceof ApiError?await e.readPayload():null;if(alive.current)setError(detail?.detail||detail?.validationMessages.join("; ")||(e instanceof Error?e.message:"Không thực hiện được thao tác."))},[]);
 const get=useCallback(<T,>(path:string)=>client.request<T>(base+path,{cache:"no-store"}),[client,base]);
 const mutate=useCallback(async<T,>(path:string,method:string,body?:unknown)=>{await loadCsrfToken(client);return client.request<T>(base+path,{method,...(body===undefined?{}:{body:JSON.stringify(body)})})},[client,base]);
 const refreshRooms=useCallback(async()=>{const r=await get<Items<Room>>("/rooms");if(alive.current&&!accessRevoked.current)setRooms(r.items);return r.items},[get]);
 const refreshContacts=useCallback(async()=>{const r=await get<Contacts>("/contacts");if(alive.current&&!accessRevoked.current)setContacts(r)},[get]);
 const refreshMessages=useCallback(async(id:string,read=true)=>{
  const seq=++requestSeq.current;
  const r=await get<Items<Message>>("/rooms/"+id+"/messages?limit=50");
  if(!alive.current||active.current!==id||seq!==requestSeq.current)return;
  setMessages(old=>{const first=r.items[0]?.created_at;return [...old.filter(m=>first&&m.created_at<first&&!r.items.some(n=>n.id===m.id)),...r.items]});
  if(read&&document.visibilityState==="visible")await mutate("/rooms/"+id+"/read","POST").catch(()=>{});
 },[get,mutate]);
 const choose=useCallback((id:string)=>{if(active.current===id){setTab("rooms");return}active.current=id;++requestSeq.current;setRoomId(id);setMessages([]);setPins([]);setShowPins(false);setMore(true);setDraft("");setReply(null);setMentions([]);setImages([]);setTag(false);setError("");setTab("rooms");void refreshMessages(id).then(()=>refreshRooms()).catch(report)},[refreshMessages,refreshRooms,report]);
 useEffect(()=>{alive.current=true;accessRevoked.current=false;let cancelled=false;let revoked=false;let timer:ReturnType<typeof setTimeout>|undefined;
  const sync=()=>{if(cancelled||revoked||accessRevoked.current||document.visibilityState!=="visible")return;void refreshRooms().then(rs=>{if(cancelled||revoked)return;if(!active.current&&rs.length)choose(rs.find(r=>r.kind==="org")?.id??rs[0].id);else if(active.current&&!rs.some(r=>r.id===active.current)){active.current="";setRoomId("");setMessages([])}else if(active.current)void refreshMessages(active.current,Boolean(rs.find(r=>r.id===active.current)?.unread)).catch(report)}).catch(report)};
  const es=new EventSource(client.url(base+"/events"),{withCredentials:true});
  es.addEventListener("ready",()=>{setConnected(true);sync()});
  es.addEventListener("chat",()=>{clearTimeout(timer);timer=setTimeout(sync,250)});
  es.addEventListener("contacts",()=>{void refreshContacts().catch(report);void refreshRooms().catch(report)});
  es.addEventListener("revoked",()=>{revoked=true;accessRevoked.current=true;++requestSeq.current;++rosterSeq.current;es.close();setConnected(false);setRooms([]);setMessages([]);setPins([]);setPeople([]);setContacts(emptyContacts);setDraft("");setImages([]);setMentions([]);setReply(null);active.current="";setRoomId("");setError("Quyền truy cập chat đã bị thu hồi.")});
  es.addEventListener("unavailable",()=>{setConnected(false);es.close()});
  es.onerror=()=>setConnected(false);
  const poll=setInterval(sync,30000);document.addEventListener("visibilitychange",sync);sync();
  return()=>{cancelled=true;alive.current=false;es.close();clearTimeout(timer);clearInterval(poll);document.removeEventListener("visibilitychange",sync)};
 },[base,client,choose,refreshRooms,refreshMessages,refreshContacts,report]);
 useEffect(()=>{if(tab!=="people"&&tab!=="group"&&!tag)return;const seq=++rosterSeq.current;const timer=setTimeout(()=>{void get<Items<Person>>("/people?q="+encodeURIComponent(q.slice(0,80))).then(r=>{if(alive.current&&!accessRevoked.current&&seq===rosterSeq.current)setPeople(r.items)}).catch(report)},250);return()=>{clearTimeout(timer);++rosterSeq.current}},[tab,tag,q,get,report]);
 useEffect(()=>{if(tab==="contacts")void refreshContacts().catch(report)},[tab,refreshContacts,report]);
 const last=messages.at(-1)?.id;useEffect(()=>{scroller.current?.scrollTo({top:scroller.current.scrollHeight})},[last,roomId]);
 const run=async(fn:()=>Promise<unknown>)=>{if(sending.current||accessRevoked.current)return;sending.current=true;setBusy(true);setError("");try{await fn()}catch(e){await report(e)}finally{sending.current=false;if(alive.current)setBusy(false)}};
 const direct=(id:string)=>run(async()=>{const r=await mutate<Room>("/rooms/direct","POST",{user_id:id});await refreshRooms();choose(r.id)});
 const group=()=>run(async()=>{if(groupAction==="invite"){await mutate("/rooms/"+roomId+"/members","POST",{user_ids:groupUsers.map(p=>p.id)});await refreshRooms();setTab("rooms")}else{const r=await mutate<Room>("/rooms","POST",{name:groupName.trim(),member_ids:groupUsers.map(p=>p.id)});await refreshRooms();choose(r.id)}setGroupName("");setGroupUsers([])});
 const addImages=async(files:File[])=>{const id=active.current;try{const next=await Promise.all(files.slice(0,4-images.length).map(prepareChatImage));if(active.current!==id||!alive.current||accessRevoked.current)return;setImages(old=>[...old,...next].slice(0,4))}catch(e){await report(e)}};
 const send=()=>run(async()=>{const id=roomId;if(!id||(!draft.trim()&&!images.length))return;await mutate("/rooms/"+id+"/messages","POST",{body:draft.trim(),reply_to_id:reply?.id??null,mention_ids:mentions.map(p=>p.id),attachments:images});if(active.current!==id)return;setDraft("");setReply(null);setMentions([]);setImages([]);await refreshMessages(id);await refreshRooms()});
 const action=(m:Message,kind:"pin"|"reaction",value?:string)=>run(async()=>{const id=roomId;await mutate("/messages/"+m.id+"/"+(kind==="pin"?"pin":"reactions"),kind==="pin"&&m.pinned?"DELETE":"POST",kind==="reaction"?{emoji:value}:undefined);if(active.current!==id)return;await refreshMessages(id);if(showPins)setPins((await get<Items<Message>>("/rooms/"+id+"/pins")).items)});
 const openPins=()=>run(async()=>{if(showPins){setShowPins(false);return}const id=roomId;const p=await get<Items<Message>>("/rooms/"+id+"/pins");if(active.current===id){setPins(p.items);setShowPins(true)}});
 const older=()=>run(async()=>{const id=roomId;const first=messages[0]?.id;if(!first)return;const r=await get<Items<Message>>("/rooms/"+id+"/messages?limit=50&before="+first);if(active.current===id){setMessages(old=>[...r.items.filter(m=>!old.some(n=>n.id===m.id)),...old]);setMore(r.items.length===50)}});
 const contactAction=(path:string,method="POST",body?:unknown)=>run(async()=>{await mutate(path,method,body);await refreshContacts();setPeople((await get<Items<Person>>("/people?q="+encodeURIComponent(q.slice(0,80)))).items)});
 const showGroup=(mode:"create"|"invite")=>{setGroupAction(mode);setGroupUsers([]);setGroupName("");setQ("");setTab("group")};
 const tagPeople=selected?.kind==="direct"?people.filter(p=>p.id===selected.peer?.id):people;
 return <section className="ovg-chat-pane size-2 org-chat" aria-label="Chat tổ chức">
  <header><b>Chat tổ chức</b><small>{connected?"Realtime":"Đồng bộ định kỳ"}</small><button type="button" onClick={close} aria-label="Đóng chat tổ chức">×</button></header>
  {error&&<div role="alert" className="org-chat-error">{error}<button onClick={()=>setError("")}>×</button></div>}
  <div className="org-chat-layout"><aside className="org-chat-sidebar">
   <input value={roomSearch} onChange={e=>setRoomSearch(e.target.value)} placeholder="Tìm cuộc trò chuyện" aria-label="Tìm cuộc trò chuyện"/>
   <nav className="org-chat-tabs"><button aria-pressed={tab==="rooms"} onClick={()=>setTab("rooms")}>Phòng</button><button aria-pressed={tab==="people"} onClick={()=>{setQ("");setTab("people")}}>Tìm người</button><button aria-pressed={tab==="contacts"} onClick={()=>setTab("contacts")}>Danh bạ</button><button aria-pressed={tab==="group"} onClick={()=>showGroup("create")}>＋ Nhóm</button></nav>
   <div className="org-chat-rooms">{visibleRooms.map(r=><button key={r.id} aria-label={(r.kind==="org"?"# ":r.kind==="group"?"♟ ":"")+r.name} aria-pressed={roomId===r.id&&tab==="rooms"} disabled={busy} onClick={()=>choose(r.id)} title={r.name}><span className="org-chat-avatar">{r.kind==="org"?"#":r.kind==="group"?"♟":r.name.trim().split(/\s+/).slice(-2).map(x=>x[0]).join("").toUpperCase()}</span><span className="org-chat-room-text"><b>{r.name}</b><small>{r.last_message?r.last_message.author_name+": "+(r.last_message.body||"Ảnh"):r.kind==="org"?"Phòng tổ chức":r.kind==="group"?"Nhóm · "+r.member_count+" người":"Nhắn riêng"}</small></span>{r.unread>0&&<strong>{r.mentioned?"@":""}{r.unread>99?"99+":r.unread}</strong>}</button>)}</div>
  </aside><main className="org-chat-thread">
  {tab==="rooms"&&<>
   {selected&&<><div className="org-chat-room-head"><b>{selected.name}</b><small>{selected.member_count} người</small><button disabled={busy} onClick={()=>void openPins()}>Ghim</button>{selected.kind==="group"&&<button onClick={()=>showGroup("invite")}>Mời</button>}</div>
   {showPins&&<div className="org-chat-pins">{pins.length?pins.map(m=><div key={m.id}><b>{m.author_name}</b> {m.body||"Ảnh"}</div>):"Chưa có tin ghim"}</div>}
   <div className="ovg-chat-msgs" ref={scroller} aria-live="polite">{more&&messages.length>=50&&<button onClick={()=>void older()} disabled={busy}>Xem tin cũ</button>}{messages.map(m=><article key={m.id} className={"org-chat-message"+(m.author_id===accountId?" mine":"")}><b>{m.author_name}</b>{m.reply_to&&<blockquote>{m.reply_to.author_name}: {m.reply_to.body}</blockquote>}<p>{m.body}</p>{m.mentions.length>0&&<small>{m.mentions.map(p=>"@"+p.display_name).join(" ")}</small>}{m.attachments.map(a=><a key={a.id} href={client.url(base+"/attachments/"+a.id)} target="_blank" rel="noopener noreferrer"><img src={client.url(base+"/attachments/"+a.id)} alt="Ảnh đính kèm" loading="lazy"/></a>)}<small>{new Date(m.created_at).toLocaleTimeString("vi-VN",{hour:"2-digit",minute:"2-digit"})}{m.pinned?" · Đã ghim":""}</small><div className="org-chat-actions">{m.reactions.map(r=><button key={r.emoji} aria-pressed={r.mine} disabled={busy} onClick={()=>void action(m,"reaction",r.emoji)}>{r.emoji} {r.count}</button>)}<button disabled={busy} onClick={()=>setReply(m)}>Trả lời</button><button disabled={busy} onClick={()=>void action(m,"pin")}>{m.pinned?"Bỏ ghim":"Ghim"}</button>{emojis.filter(e=>!m.reactions.some(r=>r.emoji===e)).map(e=><button key={e} disabled={busy} onClick={()=>void action(m,"reaction",e)}>{e}</button>)}</div></article>)}{!messages.length&&<p className="ovg-live-empty">Chưa có tin nhắn.</p>}</div>
   <form className="org-chat-compose" onSubmit={e=>{e.preventDefault();void send()}} onPaste={e=>{const files=Array.from(e.clipboardData.files);if(files.length){e.preventDefault();void addImages(files)}}}>
   {reply&&<div>Trả lời {reply.author_name}: {reply.body.slice(0,70)} <button type="button" onClick={()=>setReply(null)}>×</button></div>}
   {mentions.map(p=><button type="button" key={p.id} onClick={()=>setMentions(old=>old.filter(x=>x.id!==p.id))}>@{p.display_name} ×</button>)}
   {images.map((a,i)=><button type="button" key={i} onClick={()=>setImages(old=>old.filter((_,n)=>n!==i))}><img src={a.data_url} alt="Bỏ ảnh"/> ×</button>)}
   {tag&&<div><input value={q} onChange={e=>setQ(e.target.value)} placeholder="Tìm người để tag"/><small>Chỉ tag người thuộc phòng.</small>{tagPeople.filter(p=>!mentions.some(x=>x.id===p.id)).map(p=><button type="button" key={p.id} onClick={()=>{setMentions(old=>[...old,p].slice(0,20));setTag(false)}}>{p.display_name}</button>)}</div>}
   {emoji&&<EmojiPicker close={()=>setEmoji(false)} pick={e=>setDraft(old=>(old+e).slice(0,4000))}/>}
   <textarea onKeyDown={e=>{if(e.key==="Enter"&&!e.shiftKey&&!e.nativeEvent.isComposing){e.preventDefault();void send()}}} value={draft} maxLength={4000} onChange={e=>setDraft(e.target.value)} placeholder="Nhập tin nhắn hoặc dán ảnh…" aria-label="Tin nhắn" disabled={busy}/>
   <div><button type="button" onClick={()=>setEmoji(v=>!v)}>😊</button><button type="button" onClick={()=>{setQ("");setTag(v=>!v)}}>@</button><label className="org-chat-file">Ảnh<input type="file" accept="image/png,image/jpeg,image/webp" multiple disabled={busy||images.length>=4} onChange={e=>{void addImages(Array.from(e.currentTarget.files??[]));e.currentTarget.value=""}}/></label><small>Enter gửi · Shift+Enter xuống dòng</small><button className="org-chat-send" type="submit" disabled={busy||(!draft.trim()&&!images.length)}>Gửi</button></div>
   </form></>}
  </>}
  {(tab==="people"||tab==="group")&&<div className="ovg-chat-msgs">
   {tab==="group"&&<>{groupAction==="create"&&<input value={groupName} maxLength={120} onChange={e=>setGroupName(e.target.value)} placeholder="Tên nhóm"/>}<p>{groupUsers.length}/49 người đã chọn</p>{groupUsers.map(p=><button key={p.id} onClick={()=>setGroupUsers(old=>old.filter(x=>x.id!==p.id))}>{p.display_name} ×</button>)}<button disabled={busy||!groupUsers.length||(groupAction==="create"&&!groupName.trim())} onClick={()=>void group()}>{groupAction==="create"?"Tạo nhóm":"Mời thành viên"}</button></>}
   <input value={q} onChange={e=>setQ(e.target.value)} placeholder="Tìm trong tổ chức" aria-label="Tìm người"/>
   {people.map(p=><div className="org-chat-person" key={p.id}><b>{p.display_name}</b>{tab==="group"?<button disabled={groupUsers.length>=49&&!groupUsers.some(x=>x.id===p.id)} onClick={()=>setGroupUsers(old=>old.some(x=>x.id===p.id)?old.filter(x=>x.id!==p.id):[...old,p])}>{groupUsers.some(x=>x.id===p.id)?"Đã chọn":"Chọn"}</button>:<><button disabled={busy} onClick={()=>void direct(p.id)}>Nhắn</button><button disabled={busy||p.contact||p.request==="outgoing"} onClick={()=>void contactAction("/contacts/requests","POST",{user_id:p.id})}>{p.contact?"Bạn bè":p.request==="outgoing"?"Đã gửi":p.request==="incoming"?"Chấp nhận":"Kết bạn"}</button></>}</div>)}
  </div>}
  {tab==="contacts"&&<div className="ovg-chat-msgs"><b>Lời mời đến</b>{contacts.incoming.map(p=><div className="org-chat-person" key={p.id}>{p.display_name}<button disabled={busy} onClick={()=>void contactAction("/contacts/requests/"+p.id+"/accept")}>Đồng ý</button><button disabled={busy} onClick={()=>void contactAction("/contacts/requests/"+p.id+"/decline")}>Từ chối</button></div>)}<b>Đã gửi</b>{contacts.outgoing.map(p=><p key={p.id}>{p.display_name}</p>)}<b>Bạn bè</b>{contacts.items.map(p=><div className="org-chat-person" key={p.user_id}>{p.display_name}<button disabled={busy} onClick={()=>void direct(p.user_id)}>Nhắn</button><button disabled={busy} onClick={()=>void contactAction("/contacts/"+p.user_id,"DELETE")}>Xóa</button></div>)}</div>}
 </main></div>
 </section>;
}
