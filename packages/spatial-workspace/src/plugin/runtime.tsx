import React, {memo, useLayoutEffect, useMemo, useRef, useState, useCallback} from "react";
import type {StudioContribution, StudioHost, StudioPlugin} from "./studio-contract";

export const EMPTY_CONTRIBUTION: StudioContribution = Object.freeze({});
export type ContributionSnapshot = {plugin: StudioPlugin; contribution: StudioContribution};
type Props = {plugin: StudioPlugin; host: StudioHost; publish: (snapshot: ContributionSnapshot) => void; onError: (plugin: StudioPlugin, error: unknown) => void};

// Only this child runs the plugin's hook. Its key can change independently of the
// Core map. Memoization and a stable host prevent publication -> render loops.
const Runtime = memo(function Runtime({plugin,host,publish}:Props){
  const useStudio = plugin.useStudio;
  const contribution = useStudio(host);
  useLayoutEffect(()=>{publish({plugin,contribution});});
  return null;
});
class RuntimeBoundary extends React.Component<Props,{failed:boolean}>{
  state = {failed:false};
  static getDerivedStateFromError(){return {failed:true};}
  componentDidCatch(error:unknown){this.props.onError(this.props.plugin,error);}
  render(){return this.state.failed ? null : <Runtime {...this.props}/>;}
}
export const StudioRuntime = memo(RuntimeBoundary);

export type ResidentPlugin={plugin:StudioPlugin;instanceKey:string};
type ResidentProps={resident:ResidentPlugin;host:StudioHost;foreground:boolean;publish:(key:string,value:ContributionSnapshot)=>void;onError:(key:string,plugin:StudioPlugin,error:unknown)=>void};
const ResidentRuntime=memo(function ResidentRuntime({resident,host,foreground,publish,onError}:ResidentProps){
  const token=useMemo(()=>({}),[foreground]);
  const active=useRef<object|null>(null);
  useLayoutEffect(()=>{active.current=foreground?token:null;return()=>{active.current=null}},[foreground,token]);
  const allowed=()=>active.current===token;
  const guardedHost=useMemo<StudioHost>(()=>({...host,foreground,
    action:async task=>{if(!allowed())throw Error("Module is not the foreground workspace");await host.action(task)},
    select:key=>{if(allowed())host.select(key)},goto:key=>{if(allowed())host.goto(key)},
    fit:()=>{if(allowed())host.fit()},resetLayers:()=>{if(allowed())host.resetLayers()},
    notify:message=>{if(allowed())host.notify(message)},
  }),[host,foreground,token]);
  const emit=useCallback((snapshot:ContributionSnapshot)=>publish(resident.instanceKey,snapshot),[resident.instanceKey,publish]);
  const fail=useCallback((plugin:StudioPlugin,error:unknown)=>{active.current=null;onError(resident.instanceKey,plugin,error)},[resident.instanceKey,onError]);
  return <StudioRuntime plugin={resident.plugin} host={guardedHost} publish={emit} onError={fail}/>;
});

/** Keep each activated module's hook alive. Only one selected contribution may
 * own global map styling, filtering, panels and commands at a time. */
export function useStudioFleet(residents:readonly ResidentPlugin[],foregroundId:string,host:StudioHost){
  const [snapshots,setSnapshots]=useState<Record<string,ContributionSnapshot>>({});
  const [failures,setFailures]=useState<Record<string,string>>({});
  const publish=useCallback((key:string,value:ContributionSnapshot)=>setSnapshots(old=>({...old,[key]:value})),[]);
  const onError=useCallback((key:string,_plugin:StudioPlugin,error:unknown)=>{
    setSnapshots(old=>{const next={...old};delete next[key];return next});
    setFailures(old=>({...old,[key]:error instanceof Error?error.message:String(error)}));
  },[]);
  const identity=residents.map(r=>r.instanceKey).join("|");
  useLayoutEffect(()=>{
    const keys=new Set(residents.map(r=>r.instanceKey));
    const prune=<T,>(old:Record<string,T>)=>Object.keys(old).some(k=>!keys.has(k))?Object.fromEntries(Object.entries(old).filter(([k])=>keys.has(k))):old;
    setSnapshots(prune);setFailures(prune);
  },[identity]);
  const selected=residents.find(r=>r.plugin.id===foregroundId);
  const snapshot=selected&&snapshots[selected.instanceKey];
  const failure=selected?failures[selected.instanceKey]:undefined;
  const contribution=!failure&&selected&&snapshot&&snapshot.plugin===selected.plugin?snapshot.contribution:EMPTY_CONTRIBUTION;
  return {contribution,failure,runtimes:residents.map(r=><ResidentRuntime key={r.instanceKey} resident={r} host={host} foreground={r.plugin.id===foregroundId} publish={publish} onError={onError}/>)};
}
