/* Standalone public-page patch bridge. Normal builds use LandingSwarm directly.
 * This entry lets the public landing be updated independently of the GIS app bundle.
 */
import {createRoot,type Root} from "react-dom/client";
import {LandingSwarm} from "./LandingSwarm";
let host:HTMLDivElement|null=null;
let root:Root|null=null;
let page:Element|null=null;
let enabled:boolean|undefined;
function sync(){
 const landing=document.querySelector(".landing-page");
 if(landing!==page){root?.unmount();host?.remove();root=null;host=null;page=landing;enabled=undefined;}
 if(!landing)return;
 if(!host){
  if(landing.querySelector(".landing-swarm-canvas"))return;
  host=document.createElement("div");host.dataset.swarmBridge="1";landing.append(host);root=createRoot(host);
 }
 const next=!landing.querySelector("dialog[open],.landing-invite")&&!location.hash.includes("invitation_id")&&!location.hash.includes("secret=");
 if(enabled!==next){enabled=next;root?.render(<LandingSwarm enabled={next}/>)}
}
if(location.pathname==="/"){const observer=new MutationObserver(sync);observer.observe(document.body,{childList:true,subtree:true,attributes:true,attributeFilter:["open"]});sync();}
