import {useEffect,useState} from "react";
import {ApiClient} from "@openvietgis/client-sdk";

// Spec 058: project tasks for map pins, layer counts and the feature inspector.
// Layer-level tasks: the server pairs source_id with feature_id, so a layer task carries the marker
// "⟦layer:<layer id>⟧" in its description. shortcut: move to a real layer_id column when tasks get a spec.
export type ProjectTask={id:string;title:string;description:string;status:"todo"|"in_progress"|"done";priority:string;assignee_id:string;assignee_name:string;creator_name:string;due_date:string|null;longitude:number|null;latitude:number|null;source_id?:string|null;feature_id?:string|null};
export const LAYER_TAG=/⟦layer:([\w-]{1,64})⟧/;
export const taskLayerId=(t:ProjectTask)=>LAYER_TAG.exec(t.description)?.[1]??null;
export const isLate=(t:ProjectTask,today=new Date().toLocaleDateString("sv-SE"))=>t.status!=="done"&&!!t.due_date&&t.due_date<today;
export type NewTask={feature?:{source_id:string;feature_id:string;label?:string};layer?:{id:string;name:string};position?:{longitude:number;latitude:number}};
export const newTask=(d:NewTask)=>window.dispatchEvent(new CustomEvent("ovg:new-task",{detail:d}));
export function useProjectTasks(client:ApiClient,org:string,pid:string,enabled:boolean){
 const [tasks,setTasks]=useState<ProjectTask[]>([]);
 useEffect(()=>{if(!enabled){setTasks([]);return}let alive=true;
  const load=async()=>{try{const r=await client.request<{items:ProjectTask[]}>("/organizations/"+org+"/projects/"+pid+"/collaboration/tasks?limit=200",{cache:"no-store"});if(alive)setTasks(r.items)}catch{/* keep the last list */}};
  void load();const t=setInterval(()=>void load(),60000),on=()=>void load();window.addEventListener("ovg:tasks-changed",on);
  return()=>{alive=false;clearInterval(t);window.removeEventListener("ovg:tasks-changed",on)}},[org,pid,enabled]);
 return tasks;
}
