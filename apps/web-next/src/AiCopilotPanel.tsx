import {useEffect,useRef,type RefObject} from "react";
import {mountAiCopilot} from "./ai-copilot/first-party.js";
import "./ai-copilot/first-party.css";
import type {PartnerMapBridge} from "./partner-map-bridge";

/**
 * The core is not coupled to the AI relay process. A deployment that has
 * not published valid same-origin OAuth resource metadata remains hidden.
 * Project eligibility is enforced by the caller and revalidated by the relay.
 */
export function AiCopilotPanel({org,pid,bridge}:{org:string;pid:string;bridge:RefObject<PartnerMapBridge|null>}) {
 const host=useRef<HTMLSpanElement>(null);
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
       unmount=mountAiCopilot(host.current,{organizationId:org,projectId:pid,getBridge:()=>bridge.current});
     }catch{/* Copilot remains unavailable until the approved service is ready. */}
   })();
   return()=>{controller.abort();unmount?.()};
 },[org,pid,bridge]);
 return <span ref={host} className="ai-copilot-host"/>;
}
