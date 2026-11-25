import React,{useEffect,useState} from "react";
import {ApiClient} from "@openvietgis/client-sdk";

// Spec 058: remind me of my tasks that are due today, tomorrow or overdue (checked every 5 minutes;
// each task is announced once per day; a browser notification too when the person allowed it).
type Task={id:string;title:string;status:string;due_date:string|null;creator_name:string};
const today=()=>new Date().toLocaleDateString("sv-SE");
export const dueLabel=(due:string,now=today())=>{const d=Math.round((Date.parse(due)-Date.parse(now))/86400000);return d<0?"quá hạn "+(-d)+" ngày":d===0?"đến hạn hôm nay":d===1?"đến hạn ngày mai":"còn "+d+" ngày"};
export function TaskReminder({client,org,pid,open}:{client:ApiClient;org:string;pid:string;open:()=>void}){
 const [due,setDue]=useState<Task[]>([]),[toast,setToast]=useState<Task[]>([]);
 useEffect(()=>{let alive=true;const key="ovg:reminded:"+pid+":"+today();
  const check=async()=>{try{const r=await client.request<{items:Task[]}>("/organizations/"+org+"/projects/"+pid+"/collaboration/tasks?mine=true",{cache:"no-store"});if(!alive)return;
   const soon=r.items.filter(t=>t.status!=="done"&&t.due_date&&Date.parse(t.due_date)-Date.parse(today())<=86400000);setDue(soon);
   let seen:string[]=[];try{seen=JSON.parse(localStorage.getItem(key)??"[]")}catch{/* storage unavailable */}
   const fresh=soon.filter(t=>!seen.includes(t.id));if(!fresh.length)return;setToast(fresh);
   try{localStorage.setItem(key,JSON.stringify([...seen,...fresh.map(t=>t.id)]))}catch{/* storage unavailable */}
   if("Notification" in window&&Notification.permission==="granted")for(const t of fresh.slice(0,3))new Notification("Nhắc việc: "+t.title,{body:dueLabel(t.due_date!)+" · giao bởi "+t.creator_name,tag:"ovg-task-"+t.id})}catch{/* offline: try again later */}};
  void check();const timer=setInterval(()=>void check(),300000);return()=>{alive=false;clearInterval(timer)}},[org,pid]);
 return <>
  <button type="button" className="plugin-tool ovg-reminder" title={due.length?due.length+" việc sắp/đã đến hạn":"Không có việc đến hạn"} onClick={()=>{if("Notification" in window&&Notification.permission==="default")void Notification.requestPermission();open()}}>⏰{due.length?<b className="ovg-chat-badge">{due.length}</b>:null}</button>
  {toast.length>0&&<div className="ovg-reminder-toast" role="status"><b>⏰ Nhắc việc</b>{toast.slice(0,4).map(t=><p key={t.id}><span>{t.title}</span><small className={Date.parse(t.due_date!)<Date.parse(today())?"late":""}>{dueLabel(t.due_date!)}</small></p>)}<div><button type="button" onClick={()=>{setToast([]);open()}}>Xem việc</button><button type="button" onClick={()=>setToast([])}>Để sau</button></div></div>}
 </>;
}
