export type Capabilities = {protocol_version:number; standalone:boolean; managed:boolean; cloud:boolean; local_tiles:boolean; transform:boolean;local_edit:boolean;viewport:boolean;raster_archive:boolean};
export type ViewState={lon:number;lat:number;zoom:number;bearing:number;pitch:number};
export type LayerStyle={color:string;opacity:number};
export type ProjectState={version:1;revision:number;view:ViewState;active:string;layers:{id:string;visible:boolean;style:LayerStyle}[]};
export type AttributeField={name:string;kind:"text"|"integer"|"number"|"boolean";required:boolean;max_length:number|null;min:number|null;max:number|null};
export type LayerInfo={id:string;name:string;source_epsg:number|null;source_table:string|null};
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
  providers:()=>host().core.invoke<{id:string;format:string;modes:string[];profile:string;candidate:boolean}[]>("desktop_providers"),
  integrity:(handle:string)=>host().core.invoke<Record<string,unknown>>("desktop_integrity",{handle}),
  directOpen:(handle:string)=>host().core.invoke<{id:string;layers:{table:string;column:string;epsg:number;z:number;m:number}[]}|null>("desktop_direct_open",{handle}),
  directPage:(handle:string,grant:string,table:string,bbox:[number,number,number,number],after="")=>host().core.invoke<{data:GeoJSON.FeatureCollection;next:string|null}>("desktop_direct_page",{handle,grant,table,bbox,after}),
  directClose:(handle:string,grant:string)=>host().core.invoke<void>("desktop_direct_close",{handle,grant}),
  projectState:(handle:string)=>host().core.invoke<ProjectState>("desktop_project_state",{handle}),
  saveProjectState:(handle:string,state:ProjectState)=>host().core.invoke<ProjectState>("desktop_save_project_state",{handle,state,expected:state.revision}),
  setSchema:(handle:string,fields:AttributeField[],confirmed:boolean)=>host().core.invoke<void>("desktop_set_schema",{handle,fields,confirmed}),
  listLayers:(handle:string)=>host().core.invoke<LayerInfo[]>("desktop_list_layers",{handle}),
  createLayer:(handle:string,name:string,grant:string|null=null,table:string|null=null)=>host().core.invoke<LayerInfo>("desktop_create_layer",{handle,name,grant,table}),
  openLayer:(handle:string,layer:string)=>host().core.invoke<Opened>("desktop_open_layer",{handle,layer}),
  list:()=>host().core.invoke<Project[]>("desktop_list"),
  exportUnifiedCandidate:(handle:string)=>host().core.invoke<{layer:string;live:number;tombstones:number;journal:number;history:number;sha256:string}[]|null>("desktop_export_unified_candidate",{handle}),
  backupProject:(handle:string)=>host().core.invoke<boolean>("desktop_backup_project",{handle}),
  restoreProject:()=>host().core.invoke<Project|null>("desktop_restore_project"),
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
  importCSV:(handle:string,options:{x:string;y:string;epsg:number;delimiter:","|";"|"tab"})=>host().core.invoke<number|null>("desktop_import_csv",{handle,options}),
  compactLayer:(handle:string)=>host().core.invoke<boolean>("desktop_compact_layer",{handle}),
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

export type UnifiedFilter={op:"exists";field:string}|{op:"eq";field:string;value:unknown}|{op:"contains";field:string;text:string}|{op:"number_range";field:string;min:number;max:number};
export type UnifiedPlan={layer:string;bbox:[number,number,number,number];filters:UnifiedFilter[];relation:{relation:string;source:string}|null};
export type UnifiedCursor={revision:string;plan_digest:string;after:string};
export type UnifiedCatalog={revision:string;layers:{id:string;name:string;fields:AttributeField[]|null}[];relations:{id:string;source:string;target:string;kind:string}[]};
export type UnifiedEdit={layer:string;feature:string;expected:string;geometry:GeoJSON.Geometry|null;attributes:Attributes};
export type UnifiedLink={relation:string;source:string;target:string;add:boolean};
export type UnifiedBatch={operation:string;expected_project:string;edits:UnifiedEdit[];links:UnifiedLink[]};
export type NativeProviderStatus={ogr_linked:boolean;analytics_linked:boolean;qualification:string;spatial:string;parquet:string};
export const nativeProviders={
 importOGRStream:(handle:string,index:number,name:string,expected:string,requestId:string)=>host().core.invoke<{layer:string;revision:string;count:number;digest:string}|null>("native_import_ogr_stream",{handle,index,name,expected,requestId}),
 cancel:(requestId:string)=>host().core.invoke<void>("desktop_cancel_resource",{requestId}),
 status:()=>host().core.invoke<NativeProviderStatus>("native_provider_status"),
 importOGR:(handle:string,layer:string,index:number,expected:string)=>host().core.invoke<{revision:string}|null>("native_import_ogr",{handle,layer,index,expected}),
 transform:(handle:string,source:number,target:number,xy:[number,number][])=>host().core.invoke<[number,number][]>("native_proj",{handle,source,target,xy}),
 pointSummary:(handle:string,layer:string,expected:string,spatial:boolean)=>host().core.invoke<Record<string,unknown>>("native_point_summary",{handle,layer,expected,spatial}),
 rasterPreview:(handle:string,band:number)=>host().core.invoke<{width:number;height:number;bands:number;epsg:number|null;geotransform:number[];sample_width:number;sample_height:number;values:(number|null)[]}|null>("native_raster_preview",{handle,band})
};
export const unified={
 importGeoJSON:(handle:string,layer:string,expected:string)=>host().core.invoke<{revision:string}|null>("unified_import_geojson",{handle,layer,expected}),
 exportGeoJSON:(handle:string,layer:string,expected:string)=>host().core.invoke<number|null>("unified_export_geojson",{handle,layer,expected}),
 backup:(handle:string,expected:string)=>host().core.invoke<boolean>("unified_backup",{handle,expected}),
 restore:()=>host().core.invoke<Project|null>("unified_restore"),
 tile:async(handle:string,layer:string,expected:string,z:number,x:number,y:number,signal?:AbortSignal):Promise<ArrayBuffer>=>{
   if(signal?.aborted)throw new DOMException("Aborted","AbortError");
   const requestId=crypto.randomUUID();
   const cancel=()=>{void host().core.invoke<void>("desktop_cancel_resource",{requestId}).catch(()=>{})};
   signal?.addEventListener("abort",cancel,{once:true});
   try{
    const raw=await host().core.invoke<ArrayBuffer|number[]>("unified_tile",{handle,layer,expected,z,x,y,requestId});
    if(signal?.aborted)throw new DOMException("Aborted","AbortError");
    const b=raw instanceof ArrayBuffer?raw:Uint8Array.from(raw).buffer;
    if(b.byteLength>4*1024*1024)throw Error("RESOURCE_LIMIT");return b;
   }finally{signal?.removeEventListener("abort",cancel)}
 },
 list:()=>host().core.invoke<Project[]>("unified_list"),
 create:()=>host().core.invoke<Project>("unified_create"),
 open:(id:string)=>host().core.invoke<Opened>("unified_open",{id}),
 close:(handle:string)=>host().core.invoke<void>("unified_close",{handle}),
 catalog:(handle:string)=>host().core.invoke<UnifiedCatalog>("unified_catalog",{handle}),
 addLayer:(handle:string,name:string,expected:string)=>host().core.invoke<string>("unified_add_layer",{handle,name,expected}),
 addRelation:(handle:string,source:string,target:string,kind:string,expected:string)=>host().core.invoke<string>("unified_add_relation",{handle,source,target,kind,expected}),
 apply:(handle:string,input:UnifiedBatch)=>host().core.invoke<{revision:string;features:[string,string,string][]}>("unified_apply",{handle,input}),
 query:(handle:string,plan:UnifiedPlan,cursor:UnifiedCursor|null,expected:string)=>host().core.invoke<{data:GeoJSON.FeatureCollection;next:UnifiedCursor|null;scanned:number}>("unified_query",{handle,plan,cursor,expected}),
 compensate:(handle:string,operation:string,expected:string)=>host().core.invoke<string>("unified_compensate",{handle,operation,expected}),
 schema:(handle:string,layer:string,fields:AttributeField[],expected:string)=>host().core.invoke<string>("unified_schema",{handle,layer,fields,expected}),
};

export type UnifiedAssetKind="cad"|"las"|"laz"|"copc"|"glb"|"mbtiles"|"tiff";
export type UnifiedAsset={id:string;kind:UnifiedAssetKind;count:number;epsg:number|null;metadata:{width:number;height:number;bands:number;epsg:number|null;geotransform:number[]}|null};
export type AssetRead={chunk?:number;bbox?:[number,number,number,number];bounds?:[number,number,number,number,number,number];level?:number;tile?:[number,number,number];band?:number;pixel?:[number,number,number,number];shape?:[number,number]};
export type PersistentAsset={id:string;kind:UnifiedAssetKind;epsg:number|null;bytes:number;sha256:string};
export const unifiedAssets={
 tiffTile:async(handle:string,id:string,band:number,z:number,x:number,y:number,stretch:[number,number],signal?:AbortSignal)=>{
  if(signal?.aborted)throw new DOMException("Aborted","AbortError");
  const requestId=crypto.randomUUID();
  const cancel=()=>{void host().core.invoke<void>("desktop_cancel_resource",{requestId}).catch(()=>{})};
  signal?.addEventListener("abort",cancel,{once:true});
  try{
   const raw=await host().core.invoke<ArrayBuffer|number[]>("unified_tiff_tile",{handle,id,band,z,x,y,stretch,requestId});
   if(signal?.aborted)throw new DOMException("Aborted","AbortError");
   const b=raw instanceof ArrayBuffer?raw:Uint8Array.from(raw).buffer;if(b.byteLength>4*1024*1024)throw Error("RESOURCE_LIMIT");return b;
  }finally{signal?.removeEventListener("abort",cancel)}
 },
 list:(handle:string)=>host().core.invoke<PersistentAsset[]>("unified_asset_list",{handle}),
 attach:(handle:string,kind:UnifiedAssetKind,epsg:number|null,expected:string)=>host().core.invoke<PersistentAsset|null>("unified_asset_attach",{handle,kind,epsg,expected}),
 reopen:(handle:string,id:string)=>host().core.invoke<UnifiedAsset>("unified_asset_reopen",{handle,id}),
 detach:(handle:string,id:string,expected:string)=>host().core.invoke<string>("unified_asset_detach",{handle,id,expected}),
 importNDJSON:(handle:string,name:string,expected:string)=>host().core.invoke<{layer:string;revision:string;count:number;digest:string}|null>("unified_import_ndjson",{handle,name,expected}),
 open:(handle:string,kind:UnifiedAssetKind,epsg:number|null)=>host().core.invoke<UnifiedAsset|null>("unified_asset_open",{handle,kind,epsg}),
 close:(handle:string,id:string)=>host().core.invoke<void>("unified_asset_close",{handle,id}),
 read:async(handle:string,id:string,q:AssetRead={}):Promise<ArrayBuffer>=>{
  const raw=await host().core.invoke<ArrayBuffer|number[]>("unified_asset_read",{handle,id,chunk:q.chunk??0,bbox:q.bbox??null,bounds:q.bounds??null,level:q.level??null,tile:q.tile??null,band:q.band??null,pixel:q.pixel??null,shape:q.shape??null});
  const b=raw instanceof ArrayBuffer?raw:Uint8Array.from(raw).buffer;if(b.byteLength>4*1024*1024)throw Error("RESOURCE_LIMIT");return b;
 },
 exportParquet:(handle:string,layer:string,expected:string)=>host().core.invoke<number|null>("native_export_parquet",{handle,layer,expected})
};

export type RemoteTiffLevel={width:number;height:number;tile_width:number;tile_height:number;columns:number;rows:number;epsg:number|null;geotransform:number[]|null;compression:number;level:number};
export type RemoteTiffInfo={identity:{bytes:number;etag:string};info:RemoteTiffLevel;overviews:RemoteTiffLevel[]};
export const remoteTiff={
 status:()=>host().core.invoke<boolean>("remote_range_status"),
 inspect:(handle:string,url:string,requestId:string)=>host().core.invoke<RemoteTiffInfo>("remote_tiff_inspect",{handle,url,requestId}),
 tile:async(handle:string,url:string,identity:RemoteTiffInfo["identity"],level:number,col:number,row:number,requestId:string)=>{
  const raw=await host().core.invoke<ArrayBuffer|number[]>("remote_tiff_tile",{handle,url,...identity,level,col,row,requestId});
  const b=raw instanceof ArrayBuffer?raw:Uint8Array.from(raw).buffer;if(b.byteLength>1048576)throw Error("RESOURCE_LIMIT");return b;
 },
 cancel:(requestId:string)=>host().core.invoke<void>("desktop_cancel_resource",{requestId})
};

export type RemoteCopcInfo={identity:{bytes:number;etag:string};info:{center:[number,number,number];halfsize:number;spacing:number;points:number;point_format:number}};
export const remoteCopc={
 status:()=>host().core.invoke<boolean>("remote_copc_status"),
 inspect:(handle:string,url:string,requestId:string)=>host().core.invoke<RemoteCopcInfo>("remote_copc_inspect",{handle,url,requestId}),
 query:async(handle:string,url:string,identity:RemoteCopcInfo["identity"],bounds:[number,number,number,number,number,number],level:number,requestId:string)=>{
  const raw=await host().core.invoke<ArrayBuffer|number[]>("remote_copc_query",{handle,url,...identity,bounds,level,requestId});
  const b=raw instanceof ArrayBuffer?raw:Uint8Array.from(raw).buffer;if(b.byteLength>2048*24||b.byteLength%24!==0)throw Error("RESOURCE_LIMIT");return b;
 },
 cancel:remoteTiff.cancel
};

export type CadEditorStatus={session:string;state:"editing"|"ready"|"recoverable";format:"dwg"|"dxf";changed:boolean;bytes:number;sha256:string};
export const nativeCad={
 editOpen:(handle:string)=>host().core.invoke<CadEditorStatus|null>("native_cad_edit_open",{handle}),
 editList:(handle:string)=>host().core.invoke<CadEditorStatus[]>("native_cad_edit_list",{handle}),
 editPoll:(handle:string,session:string)=>host().core.invoke<CadEditorStatus>("native_cad_edit_poll",{handle,session}),
 editSave:(handle:string,session:string,expected:string)=>host().core.invoke<{bytes:number;sha256:string;format:"dwg"|"dxf"}|null>("native_cad_edit_save",{handle,session,expected}),
 editDiscard:(handle:string,session:string)=>host().core.invoke<void>("native_cad_edit_discard",{handle,session}),
 status:()=>host().core.invoke<{available:boolean;source_revision:string;mode:string}>("native_cad_status"),
 convert:(handle:string,requestId:string)=>host().core.invoke<{bytes:number;sha256:string;format:"dwg"|"dxf"}|null>("native_cad_convert",{handle,requestId}),
 cancel:remoteTiff.cancel
};
