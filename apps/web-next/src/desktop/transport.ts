export type Capabilities = {protocol_version:number; standalone:boolean; managed:boolean; cloud:boolean; local_tiles:boolean; transform:boolean};
export type Project = {id:string};
export type Opened = {id:string; handle:string};
export type Poll = {status:"pending"} | {status:"complete";xy:number[]};
type Unlisten = () => void;
type Host = {core:{invoke:<T>(command:string,args?:Record<string,unknown>)=>Promise<T>};event?:{listen:<T>(event:string,fn:(event:{payload:T})=>void)=>Promise<Unlisten>}};
function host():Host {
  const h=(window as unknown as {__TAURI__?:Host}).__TAURI__;
  if(!h?.core?.invoke)throw new Error("DESKTOP_HOST_REQUIRED");
  return h;
}
// Deliberately not a general invoke facade. All exposed calls are typed and bounded by Rust.
export const desktop={
  capabilities:()=>host().core.invoke<Capabilities>("desktop_capabilities"),
  list:()=>host().core.invoke<Project[]>("desktop_list"),
  create:()=>host().core.invoke<Project>("desktop_create"),
  open:(id:string)=>host().core.invoke<Opened>("desktop_open",{id}),
  close:(handle:string)=>host().core.invoke<void>("desktop_close",{handle}),
  transform:(handle:string,source:number,target:number,xy:number[])=>{
    if(!xy.length||xy.length>8192||xy.length%2||xy.some(v=>!Number.isFinite(v)))throw new Error("INVALID_BATCH");
    return host().core.invoke<string>("desktop_start_transform",{handle,source,target,xy});
  },
  poll:(handle:string,id:string)=>host().core.invoke<Poll>("desktop_poll",{handle,id}),
  cancel:(handle:string,id:string)=>host().core.invoke<void>("desktop_cancel",{handle,id}),
  menu:(fn:(id:string)=>void)=>host().event?.listen<string>("desktop-menu",e=>fn(e.payload))??Promise.resolve(()=>{}),
};
