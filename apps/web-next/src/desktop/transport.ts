export type Capabilities = {protocol_version:number; standalone:boolean; managed:boolean; cloud:boolean; local_tiles:boolean; transform:boolean;local_edit:boolean;viewport:boolean;raster_archive:boolean};
export type Project = {id:string};
export type Opened = {id:string; handle:string};
export type Poll = {status:"pending"} | {status:"complete";xy:number[]};
export type AssetInfo={id:string;kind:"cad"|"las"|"laz"|"copc"|"glb";count:number;epsg:number|null};
export type Attributes = Record<string,unknown>;
export type FeatureRow = {fid:string;id:string;revision:string;attributes:Attributes};
export type FeatureRecord = {id:string;revision:string;attributes:Attributes;geometry:unknown};
export type Viewport = {generation:string;data:GeoJSON.FeatureCollection;truncated:boolean};
export type EditInput = {command:string;id:string;revision:string;kind:"point"|"line"|"polygon"|null;xy:number[];attributes:Attributes};
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
  restoreSnapshot:()=>host().core.invoke<Project|null>("desktop_restore_snapshot"),
  create:()=>host().core.invoke<Project>("desktop_create"),
  open:(id:string)=>host().core.invoke<Opened>("desktop_open",{id}),
  close:(handle:string)=>host().core.invoke<void>("desktop_close",{handle}),
  transform:(handle:string,source:number,target:number,xy:number[])=>{
    if(!xy.length||xy.length>8192||xy.length%2||xy.some(v=>!Number.isFinite(v)))throw new Error("INVALID_BATCH");
    return host().core.invoke<string>("desktop_start_transform",{handle,source,target,xy});
  },
  poll:(handle:string,id:string)=>host().core.invoke<Poll>("desktop_poll",{handle,id}),
  cancel:(handle:string,id:string)=>host().core.invoke<void>("desktop_cancel",{handle,id}),
  viewport:async(handle:string,bbox:[number,number,number,number]):Promise<Viewport>=>{
    const raw=await host().core.invoke<ArrayBuffer|number[]>("desktop_viewport",{handle,bbox});
    const bytes=raw instanceof ArrayBuffer?new Uint8Array(raw):new Uint8Array(raw);
    if(bytes.byteLength>2*1024*1024)throw new Error("RESOURCE_LIMIT");
    const v=JSON.parse(new TextDecoder("utf-8",{fatal:true}).decode(bytes)) as Viewport;
    if(v.data?.type!=="FeatureCollection"||!Array.isArray(v.data.features)||v.data.features.length>512||typeof v.generation!=="string"||typeof v.truncated!=="boolean")throw new Error("INVALID_VIEWPORT");
    return v;
  },
  attributes:(handle:string,after="0",limit=100,contains="")=>host().core.invoke<{generation:string;rows:FeatureRow[];next:string|null}>("desktop_attributes",{handle,after,limit,contains}),
  feature:(handle:string,id:string)=>host().core.invoke<FeatureRecord>("desktop_feature",{handle,id}),
  edit:(handle:string,edit:EditInput)=>host().core.invoke<void>("desktop_edit",{handle,edit}),
  reshape:(handle:string,id:string,revision:string,geometry:unknown)=>host().core.invoke<void>("desktop_reshape",{handle,id,revision,geometry}),
  updateAttributes:(handle:string,id:string,revision:string,attributes:Attributes)=>host().core.invoke<void>("desktop_update_attributes",{handle,id,revision,attributes}),
  history:(handle:string,redo:boolean)=>host().core.invoke<boolean>("desktop_history",{handle,redo}),
  checkpoint:(handle:string)=>host().core.invoke<void>("desktop_checkpoint",{handle}),
  generation:(handle:string)=>host().core.invoke<string>("desktop_generation",{handle}),
  inspectGPKG:(handle:string)=>host().core.invoke<{id:string;layers:{table:string;column:string;epsg:number;z:number;m:number}[]}|null>("desktop_inspect_gpkg",{handle}),
  importLayer:(handle:string,grant:string,table:string)=>host().core.invoke<number>("desktop_import_layer",{handle,grant,table}),
  importGPKG:(handle:string)=>host().core.invoke<number|null>("desktop_import_gpkg",{handle}),
  importGeoJSON:(handle:string)=>host().core.invoke<number|null>("desktop_import_geojson",{handle}),
  exportFile:(handle:string,format:"geojson"|"gpkg")=>host().core.invoke<boolean>("desktop_export",{handle,format}),
  openAsset:(handle:string,kind:"cad"|"las"|"laz"|"copc"|"glb",epsg:number|null)=>host().core.invoke<AssetInfo|null>("desktop_open_asset",{handle,kind,epsg}),
  cadViewport:async(handle:string,id:string,bbox:[number,number,number,number]):Promise<GeoJSON.FeatureCollection>=>{
    const raw=await host().core.invoke<ArrayBuffer|number[]>("desktop_cad_viewport",{handle,id,bbox});
    const b=raw instanceof ArrayBuffer?new Uint8Array(raw):Uint8Array.from(raw);
    if(b.byteLength>2*1024*1024)throw Error("RESOURCE_LIMIT");
    const v=JSON.parse(new TextDecoder("utf-8",{fatal:true}).decode(b)) as GeoJSON.FeatureCollection;
    if(v.type!=="FeatureCollection"||!Array.isArray(v.features)||v.features.length>512)throw Error("INVALID_VIEWPORT");
    return v;
  },
  copcQuery:async(handle:string,id:string,bounds:[number,number,number,number,number,number],level:number)=>{
    const raw=await host().core.invoke<ArrayBuffer|number[]>("desktop_copc_query",{handle,id,bounds,level});
    const b=raw instanceof ArrayBuffer?raw:Uint8Array.from(raw).buffer;
    if(b.byteLength>2048*24||b.byteLength%24)throw Error("INVALID_POINT_CHUNK");
    return b;
  },
  closeAsset:(handle:string,id:string)=>host().core.invoke<void>("desktop_close_asset",{handle,id}),
  openRaster:(handle:string)=>host().core.invoke<{id:string;attribution:string}|null>("desktop_open_raster",{handle}),
  closeRaster:(handle:string)=>host().core.invoke<void>("desktop_close_raster",{handle}),
  resource:async(uri:string,signal?:AbortSignal):Promise<ArrayBuffer>=>{
    if(uri.length>512||!uri.startsWith("ovg://v1/"))throw Error("INVALID_RESOURCE");
    if(signal?.aborted)throw new DOMException("Aborted","AbortError");
    const requestId=crypto.randomUUID();
    const cancel=()=>{void host().core.invoke<void>("desktop_cancel_resource",{requestId}).catch(()=>{})};
    signal?.addEventListener("abort",cancel,{once:true});
    let raw:ArrayBuffer|number[];
    try{raw=await host().core.invoke<ArrayBuffer|number[]>("desktop_resource",{uri,requestId});}
    finally{signal?.removeEventListener("abort",cancel)}
    if(signal?.aborted)throw new DOMException("Aborted","AbortError");
    const b=raw instanceof ArrayBuffer?raw:Uint8Array.from(raw).buffer;
    if(b.byteLength>4*1024*1024)throw Error("RESOURCE_LIMIT");
    return b;
  },
  menu:(fn:(id:string)=>void)=>host().event?.listen<string>("desktop-menu",e=>fn(e.payload))??Promise.resolve(()=>{}),
};
