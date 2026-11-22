import {useCallback,useEffect,useRef,useState,type FormEvent} from "react";
import {ApiClient,ApiError,loadCsrfToken} from "@openvietgis/client-sdk";
import "./team-collaboration.css";

type Status="todo"|"in_progress"|"done";
type Priority="low"|"normal"|"high";
type Message={id:string;author_id:string;author_name:string;body:string;created_at:string};
type Member={id:string;display_name:string};
type Task={id:string;title:string;description:string;status:Status;priority:Priority;creator_id:string;creator_name:string;assignee_id:string;assignee_name:string;due_date:string|null;longitude:number|null;latitude:number|null;revision:number};
type Position={longitude:number;latitude:number};
type Props={client:ApiClient;org:string;pid:string;accountId:string;canWrite:boolean;getPosition:()=>Position|null;navigate:(p:Position)=>void};
const statuses:Record<Status,string>={todo:"Cần làm",in_progress:"Đang làm",done:"Hoàn thành"};
const priorities:Record<Priority,string>={low:"Thấp",normal:"Bình thường",high:"Cao"};
function errorText(e:unknown){return e instanceof ApiError?({401:"Phiên đăng nhập đã hết hạn.",403:"Bạn không có quyền trong dự án.",409:"Công việc đã thay đổi. Vui lòng tải lại.",422:"Dữ liệu hoặc người nhận việc không hợp lệ."} as Record<number,string>)[e.status]??"Lỗi máy chủ: "+e.status:e instanceof Error?e.message:"Không thể kết nối."}

export function TeamCollaboration({client,org,pid,accountId,canWrite,getPosition,navigate}:Props){
 const [open,setOpen]=useState(false),[tab,setTab]=useState<"chat"|"tasks"|"mine">("chat"),[messages,setMessages]=useState<Message[]>([]),[tasks,setTasks]=useState<Task[]>([]),[members,setMembers]=useState<Member[]>([]);
 const [body,setBody]=useState(""),[title,setTitle]=useState(""),[description,setDescription]=useState(""),[assignee,setAssignee]=useState(accountId),[due,setDue]=useState(""),[priority,setPriority]=useState<Priority>("normal"),[position,setPosition]=useState<Position|null>(null),[creating,setCreating]=useState(false),[busy,setBusy]=useState(false),[error,setError]=useState(""),[loaded,setLoaded]=useState(false);
 const base="/organizations/"+org+"/projects/"+pid+"/collaboration", generation=useRef(0),inFlight=useRef(false),scroll=useRef<HTMLDivElement|null>(null),first=useRef(true),lastMessage=useRef("");
 const refresh=useCallback(async(signal?:AbortSignal)=>{
  const current=++generation.current;
  try{
   const [m,t,people]=await Promise.all([client.request<{items:Message[]}>(base+"/messages",{signal}),client.request<{items:Task[]}>(base+"/tasks"+(tab==="mine"?"?mine=true":""),{signal}),canWrite?client.request<{items:Member[]}>(base+"/members",{signal}):Promise.resolve({items:[] as Member[]})]);
   if(signal?.aborted||current!==generation.current)return;
   setMessages(m.items);setTasks(t.items);setMembers(people.items);setLoaded(true);setError("");
  }catch(e){if(signal?.aborted||current!==generation.current)return;if(e instanceof ApiError&&(e.status===401||e.status===403)){setMessages([]);setTasks([]);setMembers([])}setError(errorText(e))}
 },[base,client,canWrite,tab]);
 useEffect(()=>{if(!open)return;const ac=new AbortController();setLoaded(false);void refresh(ac.signal);const timer=window.setInterval(()=>{if(!inFlight.current)void refresh(ac.signal)},5000);return()=>{ac.abort();generation.current++;window.clearInterval(timer)}},[open,refresh]);
 useEffect(()=>{if(!open||tab!=="chat")return;const last=messages.at(-1)?.id??"";const el=scroll.current;const nearBottom=!!el&&el.scrollHeight-el.scrollTop-el.clientHeight<80;if(el&&(first.current||nearBottom)&&last!==lastMessage.current)el.scrollTop=el.scrollHeight;first.current=false;lastMessage.current=last},[messages,open,tab]);
 useEffect(()=>{if(!open||!canWrite){setCreating(false);setPosition(null)}},[open,canWrite]);
 async function mutate(fn:()=>Promise<unknown>,done?:()=>void){
  if(!canWrite||inFlight.current)return;inFlight.current=true;setBusy(true);setError("");generation.current++;
  try{await loadCsrfToken(client);await fn();done?.();await refresh()}catch(e){setError(errorText(e));if(e instanceof ApiError&&e.status===409)await refresh()}finally{setBusy(false);inFlight.current=false}
 }
 function send(e:FormEvent){e.preventDefault();if(!body.trim())return;void mutate(()=>client.request(base+"/messages",{method:"POST",body:JSON.stringify({body:body.trim()})}),()=>setBody(""))}
 function create(e:FormEvent){e.preventDefault();if(!title.trim()||!assignee)return;void mutate(()=>client.request(base+"/tasks",{method:"POST",body:JSON.stringify({title:title.trim(),description,assignee_id:assignee,priority,due_date:due||null,...(position??{})})}),()=>{setTitle("");setDescription("");setDue("");setPriority("normal");setPosition(null);setCreating(false)})}
 function change(task:Task,status:Status){if(status===task.status)return;void mutate(()=>client.request(base+"/tasks/"+task.id,{method:"PATCH",body:JSON.stringify({status,revision:task.revision})}))}
 function attach(){try{const p=getPosition();if(!p)throw Error("Bản đồ chưa sẵn sàng.");setPosition(p);setError("")}catch(e){setError(errorText(e))}}
 const switchTab=(next:"chat"|"tasks"|"mine")=>{if(next===tab)return;setTab(next);setTasks([]);setLoaded(false);setCreating(false);first.current=true};
 return <><button type="button" className="plugin-tool team-collaboration-toggle" aria-expanded={open} onClick={()=>{first.current=true;setOpen(v=>!v)}} title="Chat và giao công việc của dự án">💬 Cộng tác</button>
 {open&&<aside className="team-collaboration" aria-label="Cộng tác dự án"><header><div><strong>Cộng tác dự án</strong><small>Trao đổi · Giao việc · Theo dõi</small></div><button type="button" aria-label="Đóng cộng tác" onClick={()=>setOpen(false)}>×</button></header>
 <nav aria-label="Cộng tác">{([["chat","Tin nhắn"],["tasks","Công việc"],["mine","Việc của tôi"]] as const).map(([id,label])=><button type="button" key={id} aria-pressed={tab===id} className={tab===id?"active":""} onClick={()=>switchTab(id)}>{label}</button>)}</nav>
 <div className="team-collab-sub"><span>{tab==="chat"?"Trò chuyện trong dự án":tasks.filter(t=>t.status!=="done").length+" việc đang mở"}</span><button type="button" onClick={()=>void refresh()} disabled={busy}>↻ Cập nhật</button></div>
 {error&&<p className="team-collab-error" role="alert">{error}</p>}
 {tab==="chat"?<>
 <div className="team-collab-messages" ref={scroll} role="log" aria-live="polite">{!loaded&&!messages.length?<p className="team-collab-empty">Đang tải tin nhắn…</p>:!messages.length?<p className="team-collab-empty">Chưa có tin nhắn. Bắt đầu trao đổi với nhóm.</p>:messages.map(m=><article key={m.id} className={"team-collab-message "+(m.author_id===accountId?"mine":"")}><div><b>{m.author_name}</b><time>{new Date(m.created_at).toLocaleString("vi-VN",{dateStyle:"short",timeStyle:"short"})}</time></div><p>{m.body}</p></article>)}</div>
 {canWrite?<form className="team-collab-compose" onSubmit={send}><textarea aria-label="Tin nhắn" placeholder="Nhập tin nhắn…" maxLength={4000} value={body} onChange={e=>setBody(e.target.value)} rows={2}/><button type="submit" className="primary" disabled={busy||!body.trim()}>Gửi</button></form>:<p className="team-collab-empty">Bạn có quyền xem.</p>}
 </>:<>
 {canWrite&&<div className="team-collab-new"><button type="button" className="primary" onClick={()=>{setCreating(v=>!v);setAssignee(members.some(m=>m.id===accountId)?accountId:members[0]?.id??"")}}>{creating?"Đóng biểu mẫu":"+ Giao việc"}</button></div>}
 {creating&&canWrite&&<form className="team-collab-task-form" onSubmit={create}><label>Tiêu đề<input value={title} onChange={e=>setTitle(e.target.value)} maxLength={200} required placeholder="Ví dụ: kiểm tra ranh giới lô 4"/></label><label>Mô tả<textarea value={description} onChange={e=>setDescription(e.target.value)} maxLength={2000} rows={2}/></label><label>Giao cho<select value={assignee} onChange={e=>setAssignee(e.target.value)} required><option value="">Chọn thành viên</option>{members.map(m=><option key={m.id} value={m.id}>{m.display_name}{m.id===accountId?" (Tôi)":""}</option>)}</select></label><div className="team-collab-grid"><label>Hạn hoàn thành<input type="date" value={due} onChange={e=>setDue(e.target.value)}/></label><label>Ưu tiên<select value={priority} onChange={e=>setPriority(e.target.value as Priority)}>{Object.entries(priorities).map(([k,v])=><option key={k} value={k}>{v}</option>)}</select></label></div><div className="team-collab-position">{position?<><small>📍 {position.latitude.toFixed(5)}, {position.longitude.toFixed(5)}</small><button type="button" onClick={()=>setPosition(null)}>Bỏ ghim</button></>:<button type="button" onClick={attach}>📍 Ghim tâm bản đồ hiện tại</button>}</div><button type="submit" className="primary" disabled={busy||!title.trim()||!members.some(m=>m.id===assignee)}>Tạo công việc</button></form>}
 <div className="team-collab-tasks">{!loaded&&!tasks.length?<p className="team-collab-empty">Đang tải công việc…</p>:!tasks.length?<p className="team-collab-empty">Chưa có công việc.</p>:tasks.map(t=><article className="team-collab-task" key={t.id}><div className="team-collab-task-title"><b>{t.title}</b><span className={"team-priority "+t.priority}>{priorities[t.priority]}</span></div>{t.description&&<p>{t.description}</p>}<small>Giao cho: {t.assignee_name} · Tạo bởi: {t.creator_name}</small>{t.due_date&&<small>Hạn: {t.due_date}</small>}<div className="team-collab-task-actions">{canWrite&&(accountId===t.assignee_id||accountId===t.creator_id)?<select aria-label={"Trạng thái "+t.title} value={t.status} disabled={busy} onChange={e=>change(t,e.target.value as Status)}>{Object.entries(statuses).map(([k,v])=><option key={k} value={k}>{v}</option>)}</select>:<span>{statuses[t.status]}</span>}{t.longitude!==null&&t.latitude!==null&&<button type="button" onClick={()=>{try{navigate({longitude:t.longitude!,latitude:t.latitude!})}catch(e){setError(errorText(e))}}}>📍 Đến vị trí</button>}</div></article>)}</div>
 </>}</aside>}</>
}
