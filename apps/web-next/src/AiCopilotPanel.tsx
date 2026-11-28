import {useEffect,useRef,useState,type RefObject} from "react";
import {createPortal} from "react-dom";
import {executeCadCopilot} from "./ai-copilot/cad-actions";
import {mountAiCopilot} from "./ai-copilot/first-party.js";
import "./ai-copilot/first-party.css";
import type {PartnerMapBridge} from "./partner-map-bridge";

/**
 * The core is not coupled to the AI relay process. A deployment that has
 * not published valid same-origin OAuth resource metadata remains hidden.
 * Project eligibility is enforced by the caller and revalidated by the relay.
 */
export function AiCopilotPanel({org,pid,bridge,mode}:{org:string;pid:string;bridge:RefObject<PartnerMapBridge|null>;mode:"gis"|"cad"|"3d"}) {
 const host=useRef<HTMLSpanElement>(null),modeRef=useRef(mode),modeEpoch=useRef(0);if(modeRef.current!==mode){modeRef.current=mode;modeEpoch.current++}
 const [mobile,setMobile]=useState(()=>window.matchMedia("(max-width:900px)").matches);
 useEffect(()=>{const query=window.matchMedia("(max-width:900px)");const onChange=()=>setMobile(query.matches);query.addEventListener("change",onChange);return()=>query.removeEventListener("change",onChange);},[]);
 useEffect(()=>{
   const controller=new AbortController();
   let unmount:(()=>void)|undefined;
   void (async()=>{
     try {
       const origin=window.location.origin;
       const response=await fetch("/.well-known/oauth-protected-resource/mcp",{
         credentials:"omit",cache:"no-store",redirect:"error",signal:controller.signal,
         headers:{Accept:"application/json"}
       });
       if(!response.ok||!response.headers.get("content-type")?.includes("application/json"))return;
       const metadata=await response.json();
       if(metadata.resource!==origin+"/mcp"||!Array.isArray(metadata.authorization_servers)||
          !metadata.authorization_servers.includes(origin+"/realms/openvietgis"))return;
       if(controller.signal.aborted||!host.current)return;
       unmount=mountAiCopilot(host.current,{organizationId:org,projectId:pid,getBridge:()=>bridge.current,getMode:()=>modeRef.current,getContextEpoch:()=>modeEpoch.current,runCad:(method,args)=>executeCadCopilot(org,pid,method,args)});
     }catch{/* Copilot remains unavailable until the approved service is ready. */}
   })();
   return()=>{controller.abort();unmount?.()};
 },[org,pid,bridge,mobile]);
 const element=<span ref={host} className="ai-copilot-host"/>;
 return mobile?createPortal(element,document.body):element;
}
