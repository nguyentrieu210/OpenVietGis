import React,{useCallback,useEffect,useRef,useState} from "react";
import {ApiError,getCurrentUser,listIdentityProviders,loadCsrfToken,logout,type CurrentUser,type IdentityProviderSummary} from "@openvietgis/client-sdk";
import {Brand} from "./Brand";
import {Landing,appPath} from "./Landing";
import {readInvitation,InviteAccept,clearInvitation} from "./InviteFlow";
import {captureHouseQrFocus} from "./HouseQrLink";
import {initialWorkspaceRoute} from "./route-compat";
import {observeOnlineActor} from "./workbenches/features/offline/offline-store";
import {clearSpatialCache} from "./SpatialCache";
import {clearCadDrafts} from "./CadDraftStorage";
import {apiClient as client} from "./workbenches/app/api";
import {msg} from "./api-error-message";
import {invalidateProjectAccess} from "./project-access-flight";

const studioEntryPath=()=>appPath+(new URLSearchParams(location.search).get("dockview")==="1"?"?dockview=1":"");

// Fetch the heavy spatial workspace only after identity and entry checks pass.
const Tenant = React.lazy(async()=>{
 const [{Tenant},{configureMapSurface}]=await Promise.all([import("./LiveApp"),import("./map-surface")]);
 configureMapSurface();
 return {default:Tenant};
});

export function LiveApp(){
 useEffect(()=>{captureHouseQrFocus(location.search)},[]);
 const [invitation,setInvitation]=useState(readInvitation),[preferredOrg,setPreferredOrg]=useState(initialWorkspaceRoute.org),[preferredProject,setPreferredProject]=useState(initialWorkspaceRoute.project);
 const [user,setUser]=useState<CurrentUser|null>(null),[providers,setProviders]=useState<IdentityProviderSummary[]>([]),[loading,setLoading]=useState(true),[error,setError]=useState("");
 const [workspace,setWorkspace]=useState(()=>location.pathname!=="/");
 const enter=useCallback(()=>{history.replaceState(null,"",studioEntryPath());setWorkspace(true)},[]);
 const goHome=useCallback(()=>{history.replaceState(null,"",appPath.startsWith("/devui/")?appPath:"/");setWorkspace(false)},[]);
 const sessionGeneration=useRef(0),signingOut=useRef(false),actor=useRef<string|null>(null);
 const observeActor=async(id:string|null)=>{const previous=actor.current;if(previous!==id){invalidateProjectAccess(client);actor.current=id;observeOnlineActor(id);if(previous)await Promise.all([clearSpatialCache(previous),clearCadDrafts(previous)]);return}observeOnlineActor(id)};
 const refresh=useCallback(async()=>{if(signingOut.current)return false;const generation=sessionGeneration.current;try{const who=await getCurrentUser(client);if(generation!==sessionGeneration.current||signingOut.current)return false;await observeActor(who.status==="active"?who.id:null);setUser(who);setError("");return true}catch(e){if(generation!==sessionGeneration.current||signingOut.current)return false;setUser(null);client.setCsrfToken(undefined);await observeActor(null);if(!(e instanceof ApiError&&e.status===401))setError(msg(e));return false}finally{if(generation===sessionGeneration.current&&!signingOut.current)setLoading(false)}},[]);
 const onAuth=useCallback(async()=>{if(signingOut.current)return false;const generation=sessionGeneration.current;try{const who=await getCurrentUser(client);if(generation!==sessionGeneration.current||signingOut.current)return false;await observeActor(who.status==="active"?who.id:null);setUser(who);setError("");enter();return true}catch(e){if(generation===sessionGeneration.current&&!(e instanceof ApiError&&e.status===401))setError(msg(e));return false}},[enter]);
 const signOut=async()=>{if(signingOut.current)return;signingOut.current=true;sessionGeneration.current++;invalidateProjectAccess(client);try{await loadCsrfToken(client);await logout(client);await observeActor(null);client.setCsrfToken(undefined);setUser(null);window.location.replace("/")}catch(e){signingOut.current=false;throw e}};
 useEffect(()=>{void refresh();listIdentityProviders(client).then(setProviders).catch(e=>setError(msg(e)));const check=()=>void refresh();window.addEventListener("focus",check);return()=>window.removeEventListener("focus",check)},[refresh]);
 useEffect(()=>{const handleInviteLink=()=>{const params=new URLSearchParams(location.hash.slice(1));if(params.has("secret")||params.has("invitation_id"))setInvitation(readInvitation())};window.addEventListener("hashchange",handleInviteLink);return()=>window.removeEventListener("hashchange",handleInviteLink)},[]);
 useEffect(()=>{if(user&&workspace&&!invitation&&location.pathname!==appPath)history.replaceState(null,"",studioEntryPath())},[user,workspace,invitation]);
 if(loading)return <div className="auth-complete"><Brand/><p>Đang kiểm tra phiên…</p></div>;
 if(!user||!workspace&&!invitation)return <Landing provider={providers[0]?.id} error={error} invitation={!!invitation} authenticated={!!user} onAuth={onAuth} onWorkspace={enter} onDismissInvitation={()=>{clearInvitation();setInvitation(null)}}/>;
 if(invitation)return <InviteAccept client={client} user={user} invite={invitation} onDone={(id,pid)=>{setPreferredOrg(id);setPreferredProject(pid??"");setInvitation(null);enter()}} onCancel={()=>setInvitation(null)} onSignOut={signOut}/>;
 return <React.Suspense fallback={<div className="auth-complete" role="status"><Brand/><p>Đang mở không gian làm việc…</p></div>}><Tenant key={user.id+preferredOrg} user={user} preferredOrg={preferredOrg} preferredProject={preferredProject} signOut={signOut} expire={()=>{invalidateProjectAccess(client);void observeActor(null);setUser(null);client.setCsrfToken(undefined);goHome()}}/></React.Suspense>;
}