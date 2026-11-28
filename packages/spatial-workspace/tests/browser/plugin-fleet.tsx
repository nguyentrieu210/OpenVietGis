import React,{useEffect,useMemo,useState} from "react";
import {createRoot} from "react-dom/client";
import {useStudioFleet,type ResidentPlugin} from "../../src/plugin/runtime";
import type {StudioPlugin,StudioHost} from "../../src/plugin/studio-contract";
import type {ApiClient} from "@openvietgis/client-sdk";
const stats:Record<string,number>={};
const callbacks:Record<string,()=>void>={};
const actionCallbacks:Record<string,()=>Promise<void>>={};
function sample(id:string,crash=false):StudioPlugin{return {id,useStudio(host){
 const [count,setCount]=useState(0);
 useEffect(()=>{stats[id]=(stats[id]??0)+1;return()=>{stats[id+".removed"]=(stats[id+".removed"]??0)+1}},[]);
 callbacks[id]=()=>host.select(id);actionCallbacks[id]=()=>host.action(async()=>host.select(id+"-action"));
 if(crash)throw Error("fixture failure");
 return {toolbar:<button onClick={()=>setCount(v=>v+1)}>{id} count {count}</button>,filter:row=>row.key===id,label:()=>id,mapRevision:count};
}}}
const a=sample("alpha"),b=sample("beta"),bad=sample("bad",true);
function Fixture(){
 const [ids,setIds]=useState(["alpha","beta"]),[foreground,setForeground]=useState("alpha"),[selection,setSelection]=useState(""),[outcome,setOutcome]=useState("");
 const residents=useMemo<ResidentPlugin[]>(()=>ids.map(id=>({plugin:id==="alpha"?a:id==="beta"?b:bad,instanceKey:id+":1"})),[ids]);
 const host=useMemo<StudioHost>(()=>({client:{} as ApiClient,org:"fixture",pid:"fixture",data:{layers:[],rows:[],warnings:[],permissions:[]},row:undefined,selected:"",loading:false,busy:false,editing:false,revision:0,action:async task=>task(),refresh:()=>{},fail:()=>{},select:setSelection,goto:setSelection,fit:()=>{},resetLayers:()=>{},notify:()=>{}}),[]);
 const {contribution,runtimes,failure}=useStudioFleet(residents,foreground,host);
 return <main>{runtimes}<div data-testid="map">Core map</div><button onClick={()=>setForeground("alpha")}>Alpha</button><button onClick={()=>setForeground("beta")}>Beta</button><button onClick={()=>setForeground("")}>Core</button><button onClick={()=>{setIds(v=>v.includes("bad")?v:[...v,"bad"]);setForeground("bad")}}>Crash plugin</button><button onClick={()=>setIds(v=>v.filter(i=>i!=="beta"))}>Remove beta</button><button onClick={()=>setIds(v=>v.includes("beta")?v:[...v,"beta"])}>Add beta</button><button onClick={()=>callbacks.beta?.()}>Background map call</button><button onClick={()=>void actionCallbacks.beta?.().then(()=>setOutcome("allowed"),()=>setOutcome("denied"))}>Background action</button><div data-testid="selected">{selection}</div><div data-testid="outcome">{outcome}</div><div data-testid="counts">{JSON.stringify(stats)}</div><div data-testid="owner">{contribution.label?.({} as never)??"core"}</div>{failure&&<div role="alert">{failure}</div>}{contribution.toolbar}</main>;
}
createRoot(document.querySelector("#root")!).render(<Fixture/>);
