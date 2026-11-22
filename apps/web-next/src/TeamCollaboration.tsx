import {useCallback,useEffect,useRef,useState,type FormEvent,type DragEvent} from "react";
import {ApiClient,ApiError,loadCsrfToken} from "@openvietgis/client-sdk";
import "./team-collaboration.css";

type Status="todo"|"in_progress"|"done";
type Priority="low"|"normal"|"high";
type Tab="chat"|"tasks"|"mine"|"notifications";
type Message={id:string;author_id:string;author_name:string;body:string;created_at:string;recipient_id:string|null};
type Member={id:string;display_name:string};
type Task={id:string;title:string;description:string;status:Status;priority:Priority;creator_id:string;creator_name:string;assignee_id:string;assignee_name:string;due_date:string|null;longitude:number|null;latitude:number|null;revision:number;source_message_id?:string|null};
type Notice={id:string;title:string;creator_name:string;created_at:string;read_at:string|null};
type Position={longitude:number;latitude:number};
type Props={client:ApiClient;org:string;pid:string;accountId:string;canWrite:boolean;getPosition:()=>Position|null;navigate:(p:Position)=>void};
const statuses:Record<Status,string>={todo:"Cần làm",in_progress:"Đang làm",done:"Hoàn thành"};
const priorities:Record<Priority,string>={low:"Thấp",normal:"Bình thường",high:"Cao"};
function errorText(e:unknown){return e instanceof ApiError?({401:"Phiên đăng nhập đã hết hạn.",403:"Bạn không có quyền trong dự án.",409:"Công việc đã thay đổi. Vui lòng tải lại.",422:"Dữ liệu hoặc người nhận việc không hợp lệ."} as Record<number,string>)[e.status]??"Lỗi máy chủ: "+e.status:e instanceof Error?e.message:"Không thể kết nối."}

export function TeamCollaboration({client,org,pid,accountId,canWrite,getPosition,navigate}:Props){
 const [open,setOpen]=useState(false),[tab,setTab]=useState<Tab>("chat"),[messages,setMessages]=useState<Message[]>([]),[tasks,setTasks]=useState<Task[]>([]),[members,setMembers]=useState<Member[]>([]);
 const [body,setBody]=useState(""),[title,setTitle]=useState(""),[description,setDescription]=useState(""),[assignee,setAssignee]=useState(accountId),[due,setDue]=useState(""),[priority,setPriority]=useState<Priority>("normal"),[position,setPosition]=useState<Position|null>(null),[creating,setCreating]=useState(false),[busy,setBusy]=useState(false),[error,setError]=useState(""),[loaded,setLoaded]=useState(false);
 const [peer,setPeer]=useState(""),[notices,setNotices]=useState<Notice[]>([]),[unread,setUnread]=useState(0),[live,setLive]=useState(false),[revoked,setRevoked]=useState(false),[board,setBoard]=useState(false),[source,setSource]=useState<Message|null>(null);
 const base="/organizations/"+org+"/projects/"+pid+"/collaboration", generation=useRef(0),inFlight=useRef(false),scroll=useRef<HTMLDivElement|null>(null),first=useRef(true),lastMessage=useRef(""),requestRef=useRef<AbortController|null>(null);
 const refresh=useCallback(async()=>{
  const current=++generation.current;
  requestRef.current?.abort();
  const ac=new AbortController();requestRef.current=ac;const opts={signal:ac.signal,cache:"no-store" as const};
  try{
   const n=await client.request<{items:Notice[];unread:number}>(base+"/notifications",opts);
   if(ac.signal.aborted||current!==generation.current)return;
   setNotices(n.items);setUnread(n.unread);
   if(open){
    const [m,t,people]=await Promise.all([tab==="chat"?client.request<{items:Message[]}>(base+"/messages"+(peer?"?peer="+encodeURIComponent(peer):""),opts):Promise.resolve(null),tab==="tasks"||tab==="mine"?client.request<{items:Task[]}>(base+"/tasks"+(tab==="mine"?"?mine=true":""),opts):Promise.resolve(null),canWrite?client.request<{items:Member[]}>(base+"/members",opts):Promise.resolve(null)]);
    if(ac.signal.aborted||current!==generation.current)return;
    if(m)setMessages(m.items);if(t)setTasks(t.items);if(people)setMembers(people.items);setLoaded(true);
   }
   setError("");
  }catch(e){if(ac.signal.aborted||current!==generation.current)return;if(e instanceof ApiError&&(e.status===401||e.status===403)){setMessages([]);setTasks([]);setMembers([]);setNotices([]);setUnread(0);setRevoked(true)}setError(errorText(e))}
 },[base,client,canWrite,tab,peer,open]);
 const latest=useRef(refresh);latest.current=refresh;
 // One server-push subscription per mounted project. No interval polling.
 useEffect(()=>{
  setRevoked(false);setLive(false);let timer:ReturnType<typeof setTimeout>|undefined;
  const es=new EventSource("/api/v1"+base+"/events",{withCredentials:true});
  const push=()=>{if(timer!==undefined)clearTimeout(timer);timer=setTimeout(()=>{timer=undefined;void latest.current()},50)};
  es.addEventListener("ready",()=>{setLive(true);push()});
  es.addEventListener("change",push);
  es.addEventListener("unavailable",()=>setLive(false));
  es.addEventListener("revoked",()=>{es.close();requestRef.current?.abort();generation.current++;setLive(false);setRevoked(true);setMessages([]);setTasks([]);setMembers([]);setNotices([]);setUnread(0);setBody("");setDescription("");setTitle("");setSource(null);setError("Quyền truy cập đã thay đổi. Hãy tải lại dự án.")});
  es.onerror=()=>setLive(false);
  return()=>{es.close();if(timer!==undefined)clearTimeout(timer);requestRef.current?.abort();generation.current++};
 },[base,accountId]);
 useEffect(()=>{setLoaded(false);void refresh();return()=>{requestRef.current?.abort();generation.current++}},[refresh]);
 useEffect(()=>{if(!open||tab!=="chat")return;const last=messages.at(-1)?.id??"";const el=scroll.current;const nearBottom=!!el&&el.scrollHeight-el.scrollTop-el.clientHeight<80;if(el&&(first.current||nearBottom)&&last!==lastMessage.current)el.scrollTop=el.scrollHeight;first.current=false;lastMessage.current=last},[messages,open,tab]);
 useEffect(()=>{if(!open||!canWrite){setCreating(false);setPosition(null);setSource(null)}},[open,canWrite]);
 async function mutate(fn:()=>Promise<unknown>,done?:()=>void){
  if(!canWrite||revoked||inFlight.current)return;inFlight.current=true;setBusy(true);setError("");
  try{await loadCsrfToken(client);await fn();done?.();await latest.current()}catch(e){setError(errorText(e));if(e instanceof ApiError&&e.status===409)await latest.current()}finally{setBusy(false);inFlight.current=false}
 }
 function send(e:FormEvent){e.preventDefault();if(!body.trim())return;void mutate(()=>client.request(base+"/messages",{method:"POST",body:JSON.stringify({body:body.trim(),...(peer?{recipient_id:peer}:{})})}),()=>setBody(""))}
 function create(e:FormEvent){e.preventDefault();if(!title.trim()||!assignee)return;void mutate(()=>client.request(base+"/tasks",{method:"POST",body:JSON.stringify({title:title.trim(),description,assignee_id:assignee,priority,due_date:due||null,...(position??{}),...(source?{source_message_id:source.id}:{})})}),()=>{setTitle("");setDescription("");setDue("");setPriority("normal");setPosition(null);setSource(null);setCreating(false)})}
 const editable=(task:Task)=>canWrite&&!revoked&&(accountId===task.assignee_id||accountId===task.creator_id);
 function change(task:Task,status:Status){if(status===task.status||!editable(task))return;void mutate(()=>client.request(base+"/tasks/"+task.id,{method:"PATCH",body:JSON.stringify({status,revision:task.revision})}))}
 function attach(){try{const p=getPosition();if(!p)throw Error("Bản đồ chưa sẵn sàng.");setPosition(p);setError("")}catch(e){setError(errorText(e))}}
 const switchTab=(next:Tab)=>{if(next===tab||busy)return;generation.current++;requestRef.current?.abort();setTab(next);setTasks([]);setMessages([]);setLoaded(false);setCreating(false);setSource(null);setPosition(null);first.current=true};
 function choosePeer(value:string){generation.current++;requestRef.current?.abort();setPeer(value);setLoaded(false);setMessages([]);setBody("");first.current=true}
 function newTask(m?:Message){setSource(m??null);setTitle(m?m.body.split("\n")[0].slice(0,200):"");setDescription(m?m.body.slice(0,2000):"");setDue("");setPriority("normal");setPosition(null);setAssignee(members.some(v=>v.id===accountId)?accountId:members[0]?.id??"");setCreating(true)}
 function convert(m:Message){if(peer||m.recipient_id||!canWrite||busy)return;switchTab("tasks");newTask(m)}
 function drop(e:DragEvent<HTMLDivElement>,status:Status){e.preventDefault();const id=e.dataTransfer.getData("application/x-ovg-task");const task=tasks.find(t=>t.id===id);if(task)change(task,status)}
 const taskCard=(t:Task)=><article className="team-collab-task" key={t.id} draggable={board&&editable(t)&&!busy} onDragStart={e=>{if(!editable(t))return;e.dataTransfer.setData("application/x-ovg-task",t.id);e.dataTransfer.effectAllowed="move"}}>
  <div className="team-collab-task-title"><b>{t.title}</b><span className={"team-priority "+t.priority}>{priorities[t.priority]}</span></div>{t.description&&<p>{t.description}</p>}<small>Giao cho: {t.assignee_name} · Tạo bởi: {t.creator_name}</small>{t.due_date&&<small>Hạn: {t.due_date}</small>}{t.source_message_id&&<small>↳ Tạo từ tin nhắn dự án</small>}
  <div className="team-collab-task-actions">{editable(t)?<select aria-label={"Trạng thái "+t.title} value={t.status} disabled={busy} onChange={e=>change(t,e.target.value as Status)}>{Object.entries(statuses).map(([k,v])=><option key={k} value={k}>{v}</option>)}</select>:<span>{statuses[t.status]}</span>}{t.longitude!==null&&t.latitude!==null&&<button type="button" onClick={()=>{try{navigate({longitude:t.longitude!,latitude:t.latitude!})}catch(e){setError(errorText(e))}}}>📍 Đến vị trí</button>}</div></article>;
 return <><button type="button" className="plugin-tool team-collaboration-toggle" aria-expanded={open} onClick={()=>{first.current=true;setOpen(v=>!v)}} title="Chat và giao công việc của dự án">💬 Cộng tác {unread>0&&<span className="team-collab-badge">{unread>99?"99+":unread}</span>}</button>
 {open&&<aside className={"team-collaboration "+(board&&(tab==="tasks"||tab==="mine")?"wide":"")} aria-label="Cộng tác dự án"><header><div><strong>Cộng tác dự án</strong><small><span className={live?"team-live-dot on":"team-live-dot"}/> {live?"Realtime · Đã kết nối":"Đang kết nối realtime…"}</small></div><button type="button" aria-label="Đóng cộng tác" onClick={()=>setOpen(false)}>×</button></header>
 <nav aria-label="Cộng tác">{([["chat","Tin nhắn"],["tasks","Công việc"],["mine","Việc của tôi"],["notifications",unread?"Thông báo ("+unread+")":"Thông báo"]] as const).map(([id,label])=><button type="button" key={id} disabled={busy} aria-pressed={tab===id} className={tab===id?"active":""} onClick={()=>switchTab(id)}>{label}</button>)}</nav>
 <div className="team-collab-sub"><span>{tab==="chat"?(peer?"Tin nhắn riêng":"Trò chuyện trong dự án"):tab==="notifications"?unread+" chưa đọc":tasks.filter(t=>t.status!=="done").length+" việc đang mở"}</span><button type="button" onClick={()=>void refresh()} disabled={busy||revoked}>↻ Cập nhật</button></div>
 {error&&<p className="team-collab-error" role="alert">{error}</p>}
 {revoked?<p className="team-collab-empty">Tải lại dự án để kiểm tra quyền mới.</p>:tab==="chat"?<>
 {canWrite&&<div className="team-collab-channel"><label>Cuộc trò chuyện<select aria-label="Cuộc trò chuyện" value={peer} onChange={e=>choosePeer(e.target.value)} disabled={busy}><option value="">Nhóm dự án</option>{members.filter(m=>m.id!==accountId).map(m=><option key={m.id} value={m.id}>Riêng · {m.display_name}</option>)}</select></label></div>}
 <div className="team-collab-messages" ref={scroll} role="log" aria-live="polite">{!loaded&&!messages.length?<p className="team-collab-empty">Đang tải tin nhắn…</p>:!messages.length?<p className="team-collab-empty">Chưa có tin nhắn. Bắt đầu trao đổi với nhóm.</p>:messages.map(m=><article key={m.id} className={"team-collab-message "+(m.author_id===accountId?"mine":"")}><div><b>{m.author_name}</b><time>{new Date(m.created_at).toLocaleString("vi-VN",{dateStyle:"short",timeStyle:"short"})}</time></div><p>{m.body}</p>{canWrite&&!peer&&!m.recipient_id&&<button type="button" className="team-convert" disabled={busy} onClick={()=>convert(m)}>↳ Tạo việc từ tin nhắn</button>}</article>)}</div>
 {canWrite?<form className="team-collab-compose" onSubmit={send}><textarea aria-label="Tin nhắn" placeholder="Nhập tin nhắn…" maxLength={4000} value={body} onChange={e=>setBody(e.target.value)} rows={2}/><button type="submit" className="primary" disabled={busy||!body.trim()}>Gửi</button></form>:<p className="team-collab-empty">Bạn có quyền xem.</p>}
 </>:tab==="notifications"?<div className="team-collab-tasks">{notices.length?notices.map(n=><article className={"team-collab-task "+(!n.read_at?"unread":"")} key={n.id}><b>{n.title}</b><small>{n.creator_name} đã giao việc cho bạn</small><small>{new Date(n.created_at).toLocaleString("vi-VN")}</small>{!n.read_at&&canWrite&&<button type="button" disabled={busy} onClick={()=>void mutate(()=>client.request(base+"/notifications/"+n.id+"/read",{method:"POST"}))}>Đánh dấu đã đọc</button>}<button type="button" onClick={()=>switchTab("mine")}>Mở việc của tôi</button></article>):<p className="team-collab-empty">Chưa có thông báo giao việc.</p>}</div>:<>
 <div className="team-collab-new team-collab-task-toolbar">{canWrite&&<button type="button" className="primary" onClick={()=>creating?setCreating(false):newTask()}>{creating?"Đóng biểu mẫu":"+ Giao việc"}</button>}<button type="button" aria-pressed={board} onClick={()=>setBoard(v=>!v)}>{board?"☰ Danh sách":"▦ Kanban"}</button></div>
 {creating&&canWrite&&<form className="team-collab-task-form" onSubmit={create}>{source&&<small>Đang chuyển tin nhắn dự án thành công việc. Nội dung bên dưới sẽ được chia sẻ với dự án.</small>}<label>Tiêu đề<input value={title} onChange={e=>setTitle(e.target.value)} maxLength={200} required placeholder="Ví dụ: kiểm tra ranh giới lô 4"/></label><label>Mô tả<textarea value={description} onChange={e=>setDescription(e.target.value)} maxLength={2000} rows={2}/></label><label>Giao cho<select value={assignee} onChange={e=>setAssignee(e.target.value)} required><option value="">Chọn thành viên</option>{members.map(m=><option key={m.id} value={m.id}>{m.display_name}{m.id===accountId?" (Tôi)":""}</option>)}</select></label><div className="team-collab-grid"><label>Hạn hoàn thành<input type="date" value={due} onChange={e=>setDue(e.target.value)}/></label><label>Ưu tiên<select value={priority} onChange={e=>setPriority(e.target.value as Priority)}>{Object.entries(priorities).map(([k,v])=><option key={k} value={k}>{v}</option>)}</select></label></div><div className="team-collab-position">{position?<><small>📍 {position.latitude.toFixed(5)}, {position.longitude.toFixed(5)}</small><button type="button" onClick={()=>setPosition(null)}>Bỏ ghim</button></>:<button type="button" onClick={attach}>📍 Ghim tâm bản đồ hiện tại</button>}</div><button type="submit" className="primary" disabled={busy||!title.trim()||!members.some(m=>m.id===assignee)}>Tạo công việc</button></form>}
 <div className={"team-collab-tasks "+(board?"team-kanban":"")}>{!loaded&&!tasks.length?<p className="team-collab-empty">Đang tải công việc…</p>:board?(Object.entries(statuses) as [Status,string][]).map(([k,v])=><div key={k} className="team-kanban-column" onDragOver={e=>e.preventDefault()} onDrop={e=>drop(e,k)}><h4>{v} · {tasks.filter(t=>t.status===k).length}</h4>{tasks.filter(t=>t.status===k).map(taskCard)}</div>):!tasks.length?<p className="team-collab-empty">Chưa có công việc.</p>:tasks.map(taskCard)}</div>
 </>}</aside>}</>
}
