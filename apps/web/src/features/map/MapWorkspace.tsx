import { listExtensionManifests, listProjectActivations } from "@openvietgis/client-sdk";
import {
 ApiError,MapApiError,SpatialApiError,getEffectivePermissions,getMap,getMapDisplayExtent,getMapOperation,getMapViewport,
 getProject,getSpatialFeature,getSpatialLayer,listMapHistory,listSpatialLayers,loadCsrfToken,readSpatialFeatures,saveMap,
 type MapConfiguration,type MapLayerReference,type MapViewpoint,type Project,type ProjectMap,type SaveMapInput,
 type SpatialFeatureReadContext,type MapSaveReceipt
} from "@openvietgis/client-sdk";
import { useQueries,useQuery,useQueryClient } from "@tanstack/react-query";
import { useEffect,useLayoutEffect,useRef,useState } from "react";
import { Link,useNavigate,useParams,useSearchParams } from "react-router-dom";
import { canonicalOrganizationPath,canonicalProjectPath } from "../../app/route-contract";
import { apiClient } from "../../app/api";
import { currentProtectedData,protectedQueryOptions } from "../../app/protected-query";
import { MapActivityRail } from "./MapActivityRail";
import { MapCommandLine } from "./MapCommandLine";
import { formatCoordinateInCrs,MapCrsPicker,VIETNAM_CRS_PRESETS,type DisplayCrsOption } from "./MapCrsPicker";
import { MapRibbon } from "./MapRibbon";
import { MapToolDock,type ActiveModuleSummary,type MapDockToolId } from "./MapToolDock";
import { FreshAccount } from "../auth/FreshAccount";
import { AddDataDialog } from "./AddDataDialog";
import {
 buildAccessibleMapSummary,
 type AccessibleMapState
} from "./accessibility-summary";
import { AttributeTable } from "./AttributeTable";
import { FeatureInspector } from "./FeatureInspector";
import { SaveStateIndicator } from "./SaveStateIndicator";
import { TechnicalDetailsDrawer } from "./TechnicalDetailsDrawer";
import { LayerTree } from "./LayerTree";
import { WorkspaceLayout } from "./WorkspaceLayout";
import { buildAddDataCandidates,currentAddDataCatalog,currentLayerMetadata } from "./add-data";
import { sameCamera,sameViewportBounds } from "./map-initialization";
import { buildLayerPanelItemView } from "./layer-view";
import { createCommands,invokeCommand,isCanvasCommand,type MapCommandId } from "./commands";
import { BASEMAP_OPTIONS,isBasemapAvailable,LOCAL_BASEMAP,ONLINE_BASEMAP_UNAVAILABLE,type BasemapId,type MapEngine,type RenderLayer } from "./engine";
import { loadMapEngine,type MapEngineFactory } from "./map-engine-loader";
import { createViewportFramePreparer } from "./render-frame";
import { beginSave,markUnresolved,receiptMatches,saveErrorState,type PendingSave } from "./save";
import { buildSaveIndicatorView } from "./save-indicator";
import { buildTechnicalDetailsView } from "./technical-details";
import { useUnsavedNavigationProtection } from "./unsaved-navigation";
import { applyCurrentFrameSet,blockedMapGateStatus,boundsOfGeometry,capability,combineBounds,defaultStyle,mapResponseMatches,positionCount,selectionKey,toggleSelection,viewportMatches,type Bounds,type SelectionScope } from "./workspace";
import {
 loadWorkspaceLayoutPreference,normalizeWorkspaceLayoutPreference,saveWorkspaceLayoutPreference,
 type WorkspaceLayoutPreference
} from "./workspace-layout";
import "./map.css";

const world:Bounds=[-180,-85.05112878,180,85.05112878];
const defaultCamera:MapViewpoint={longitude:0,latitude:0,zoom:1,bearing:0,pitch:0,crs:4326};
function errorMessage(error:unknown):string {
 if(error instanceof MapApiError||error instanceof SpatialApiError) {
  if(error.code==="STALE_READ_PIN")return "Nội dung hoặc lược đồ nguồn đã thay đổi. Hãy xác thực lại thông tin lớp hiện tại.";
  if(error.code==="STALE_MAP_REVISION")return "Bản đồ dùng chung đã thay đổi. Hãy tải lại phiên bản hiện tại trước khi lưu mới.";
  if(error.code==="NO_CHANGES")return "Không có thay đổi cấu hình dùng chung để lưu.";
  if(error.code==="OPERATION_INTENT_CONFLICT")return "Thao tác lưu này không khớp với ý định đã ghi nhận. Hãy tải lại trạng thái hiện tại.";
  if(error.code==="PROJECT_NOT_WRITABLE")return "Dự án hiện đang ở chế độ chỉ đọc.";
  if(error.code==="MAP_READ_UNAVAILABLE")return "Tạm thời không thể đọc dữ liệu trong khung bản đồ. Hãy xác thực lại rồi thử lại.";
  if(error.code==="TRANSFORMATION_OUT_OF_AREA")return "Hình học nằm ngoài phạm vi phép chuyển đổi tọa độ đã được phê duyệt.";
 }
 if(error instanceof ApiError&&[401,403,404].includes(error.status))return "Quyền hoặc tài nguyên hiện tại không khả dụng. Hãy xác thực lại trước khi hiển thị dữ liệu được bảo vệ.";
 return "Thao tác không khả dụng. Hệ thống không tự giả định kết quả.";
}
function requiresRevalidation(error:unknown):boolean {
 return (error instanceof ApiError&&[401,403,404].includes(error.status))||
 ((error instanceof SpatialApiError||error instanceof MapApiError)&&error.code==="STALE_READ_PIN");
}
function pinnedMatches(scope:SelectionScope,context:SpatialFeatureReadContext):boolean {
 return context.organization_id===scope.organizationId&&context.project_id===scope.projectId&&context.layer_id===scope.layerId&&context.source_id===scope.sourceId&&context.content_revision===scope.contentRevision&&context.schema_revision===scope.schemaRevision;
}
function mapStateLabel(state:string):string {
 return {
  loading:"Đang tải",
  stale:"Cần xác thực lại",
  partial:"Hiển thị một phần",
  usable:"Sẵn sàng",
  unavailable:"Không khả dụng",
  empty:"Không có dữ liệu"
 }[state]??"Đang kiểm tra";
}

function Workspace({organizationId,projectId,accountId,project,initial,canEdit,canManageAccess,createEngine}:{organizationId:string;projectId:string;accountId:string;project:Project;initial:ProjectMap;canEdit:boolean;canManageAccess:boolean;createEngine:MapEngineFactory}) {
 const navigate=useNavigate();
 const organizationPath=canonicalOrganizationPath(organizationId),projectPath=canonicalProjectPath(organizationId,projectId);
 const queryClient=useQueryClient(),canvas=useRef<HTMLDivElement>(null),engine=useRef<MapEngine|null>(null),alive=useRef(true);
 const [saved,setSaved]=useState(initial),[configuration,setConfiguration]=useState<MapConfiguration>(initial.configuration);
 const [active,setActive]=useState<string|null>(initial.configuration.layers[0]?.layer_id??null);
 const [selection,setSelection]=useState<{key:string;ids:string[];focus:string|null}|null>(null);
 const [cursor,setCursor]=useState<string|null>(null),[pages,setPages]=useState<(string|null)[]>([]);
 const [catalogCursor,setCatalogCursor]=useState<string|null>(null),[catalogPages,setCatalogPages]=useState<(string|null)[]>([]);
 const [bbox,setBbox]=useState<Bounds|null>(null),[camera,setCamera]=useState<MapViewpoint>(initial.configuration.default_viewpoint??defaultCamera);
 const [engineReady,setEngineReady]=useState(false),[engineError,setEngineError]=useState(false),[blocked,setBlocked]=useState<string|null>(null);
 const [viewpointIntent,setViewpointIntent]=useState<SaveMapInput["default_viewpoint"]>({mode:"preserve"});
 const [pending,setPending]=useState<PendingSave|null>(null),[notice,setNotice]=useState(""),[working,setWorking]=useState(false);
 const [sharedControlsOpen,setSharedControlsOpen]=useState(false);
 const [showHistory,setShowHistory]=useState(false),[historyCursor,setHistoryCursor]=useState<number|undefined>();
 const [contextMenu,setContextMenu]=useState(false),[addDataOpen,setAddDataOpen]=useState(false),[technicalDetailsOpen,setTechnicalDetailsOpen]=useState(false),[cmdLineOpen,setCmdLineOpen]=useState(true);
 const [displayCrs,setDisplayCrs]=useState<DisplayCrsOption>(()=>{try{const savedCode=Number(window.localStorage.getItem("ovg-display-crs"));return VIETNAM_CRS_PRESETS.find(c=>c.code===savedCode)??VIETNAM_CRS_PRESETS[0]!;}catch{return VIETNAM_CRS_PRESETS[0]!;}});
 const basemapDetailsRef=useRef<HTMLDetailsElement>(null);
 useEffect(()=>{const close=(ev:PointerEvent)=>{if(basemapDetailsRef.current?.open&&!basemapDetailsRef.current.contains(ev.target as Node)){basemapDetailsRef.current.open=false;}};document.addEventListener("pointerdown",close);return()=>document.removeEventListener("pointerdown",close);},[]);
 const [searchParams,setSearchParams]=useSearchParams();
 const rawTool=searchParams.get("tool");
 const activeTool=(rawTool as MapDockToolId|null)??null;
 function setActiveTool(next:MapDockToolId|null){
  const params=new URLSearchParams(searchParams);
  if(next){params.set("tool",next);}else{params.delete("tool");}
  setSearchParams(params,{replace:true});
 }
 const extActivations=useQuery({queryKey:["module-links",organizationId,projectId,"activations"],queryFn:()=>listProjectActivations(apiClient,organizationId,projectId),enabled:Boolean(organizationId&&projectId)});
 const extManifests=useQuery({queryKey:["module-links",organizationId,"manifests"],queryFn:()=>listExtensionManifests(apiClient,organizationId),enabled:Boolean(organizationId)});
 const activeModules:ActiveModuleSummary[]=(()=>{
  const activeIds=new Set((extActivations.data?.items??[]).filter(item=>item.status==="active").map(item=>item.manifest_id));
  return (extManifests.data?.items??[]).filter(m=>activeIds.has(m.id)).map(m=>{
   const routeItem=(m.ui_contributions??[]).find(c=>c.slot==="project_route");
   return {extensionKey:m.extension_key,label:typeof routeItem?.label==="string"?routeItem.label:m.display_name};
  });
 })();
 const [layoutPreference,setLayoutPreference]=useState<WorkspaceLayoutPreference>(()=>loadWorkspaceLayoutPreference());
 const defaultBasemap:BasemapId=isBasemapAvailable("osm")?"osm":LOCAL_BASEMAP;
 const [basemap,setBasemap]=useState<BasemapId>(defaultBasemap);
 useEffect(()=>{engine.current?.setBasemap(basemap);},[basemap]);
 const initialFit=useRef(false),fitGeneration=useRef(0);
 const writable=canEdit&&project.status==="active";
 const locked=working||pending?.status==="unresolved"||pending?.status==="conflict";
 const scopePrefix=["map-workspace",accountId,organizationId,projectId];
 // The full catalog is only used by Add Data. It can be expensive and duplicates
 // per-layer metadata; request it only while the dialog is open, never on ordinary map open.
 const catalogQuery=useQuery({queryKey:[...scopePrefix,"catalog",catalogCursor],queryFn:()=>listSpatialLayers(apiClient,organizationId,projectId,{limit:100,...(catalogCursor?{after_id:catalogCursor}:{})}),enabled:addDataOpen,...protectedQueryOptions});
 const catalogState=currentAddDataCatalog(addDataOpen,catalogQuery,organizationId,projectId);
 const catalog=catalogState.page;
 const metadataQueries=useQueries({queries:configuration.layers.map(ref=>({queryKey:[...scopePrefix,"layer",ref.layer_id],queryFn:()=>getSpatialLayer(apiClient,organizationId,projectId,ref.layer_id),...protectedQueryOptions}))});
 const metadataReads=metadataQueries.map((result,index)=>currentLayerMetadata(result,organizationId,projectId,configuration.layers[index]!.layer_id));
 const metadata=metadataReads.map(read=>read.layer);
 const activeIndex=configuration.layers.findIndex(ref=>ref.layer_id===active);
 const activeLayer=activeIndex>=0?metadata[activeIndex]:undefined;
 const activeScope:SelectionScope|null=activeLayer?{accountId,organizationId,projectId,layerId:activeLayer.id,sourceId:activeLayer.source_id,contentRevision:activeLayer.source.content_revision,schemaRevision:activeLayer.source.schema_revision}:null;
 const activeKey=activeScope?selectionKey(activeScope):"";
 const currentSelection=selection?.key===activeKey?selection:null;
 const selected=currentSelection?.ids??[],focus=currentSelection?.focus??null;
 const liveScope=useRef("");liveScope.current=activeKey+"|"+selected.join(",");
 const viewportQueries=useQueries({queries:configuration.layers.map((ref,index)=>{
  const layer=metadata[index],enabled=Boolean(bbox&&ref.visible&&capability(layer,"render")&&layer&&blocked!==selectionKey({accountId,organizationId,projectId,layerId:layer.id,sourceId:layer.source_id,contentRevision:layer.source.content_revision,schemaRevision:layer.source.schema_revision}));
  return {queryKey:[...scopePrefix,"viewport",ref.layer_id,layer?.source_id,layer?.source.content_revision,layer?.source.schema_revision,...(bbox??world)],
   queryFn:()=>getMapViewport(apiClient,organizationId,projectId,ref.layer_id,{content_revision:layer!.source.content_revision,schema_revision:layer!.source.schema_revision,west:bbox![0],south:bbox![1],east:bbox![2],north:bbox![3],limit:500}),enabled,...protectedQueryOptions};
 })});
 const pageQuery=useQuery({queryKey:[...scopePrefix,"page",activeKey,cursor],queryFn:()=>readSpatialFeatures(apiClient,organizationId,projectId,activeScope!.layerId,{content_revision:activeScope!.contentRevision,schema_revision:activeScope!.schemaRevision,include_geometry:false,limit:100,...(cursor?{after_feature_id:cursor}:{})}),enabled:Boolean(activeScope&&capability(activeLayer,"feature_read")&&blocked!==activeKey),...protectedQueryOptions});
 const detailQuery=useQuery({queryKey:[...scopePrefix,"detail",activeKey,focus],queryFn:()=>getSpatialFeature(apiClient,organizationId,projectId,activeScope!.layerId,focus!,{content_revision:activeScope!.contentRevision,schema_revision:activeScope!.schemaRevision,include_geometry:true,...(capability(activeLayer,"render")?{response_crs:4326}:{})}),enabled:Boolean(activeScope&&focus&&capability(activeLayer,"feature_read")&&blocked!==activeKey),...protectedQueryOptions});
 const historyQuery=useQuery({queryKey:[...scopePrefix,"history",saved.configuration_revision,historyCursor],queryFn:()=>listMapHistory(apiClient,organizationId,projectId,{limit:10,...(historyCursor!==undefined?{after_revision:historyCursor}:{})}),enabled:showHistory,...protectedQueryOptions});
 const history=currentProtectedData(historyQuery);
 const directBlocked=Boolean(activeKey&&(requiresRevalidation(pageQuery.error)||requiresRevalidation(detailQuery.error)));
 const activeBlocked=Boolean(activeKey&&(blocked===activeKey||directBlocked));
 const rawPage=currentProtectedData(pageQuery),rawDetail=currentProtectedData(detailQuery);
 const page=!activeBlocked&&activeScope&&rawPage&&pinnedMatches(activeScope,rawPage.context)&&!rawPage.context.geometry_included?rawPage:undefined;
 const detail=!activeBlocked&&activeScope&&rawDetail&&pinnedMatches(activeScope,rawDetail.context)&&rawDetail.feature.feature_id===focus?rawDetail:undefined;

 useLayoutEffect(()=>{setCursor(null);setPages([]);setSelection(null);fitGeneration.current++;},[activeKey]);
 useLayoutEffect(()=>{setTechnicalDetailsOpen(false);},[organizationId,projectId,active]);
 useEffect(()=>{if(directBlocked&&activeKey)setBlocked(activeKey);},[directBlocked,activeKey]);
 useEffect(()=>{alive.current=true;return()=>{alive.current=false;};},[]);
 function choose(id:string):void {setActive(id);setSelection(null);setCursor(null);setPages([]);setContextMenu(false);}
 function select(id:string,multiple:boolean):void {
  if(!activeScope||activeBlocked||!capability(activeLayer,"feature_read"))return;
  const ids=toggleSelection(selected,id,multiple);
  setSelection({key:activeKey,ids,focus:ids.includes(id)?id:ids[ids.length-1]??null});
 }
 const handlers=useRef({pick:select,viewport:(bounds:Bounds,value:MapViewpoint)=>{setBbox(bounds);setCamera(value);}});
 handlers.current={pick:select,viewport:(bounds,value)=>{setBbox(current=>sameViewportBounds(current,bounds)?current:bounds);setCamera(current=>sameCamera(current,value)?current:value);}};
 // Start the protected React Query observers before the synchronous renderer
 // construction/layout work. No viewport read is enabled until the real engine
 // publishes its exact bounds and the corresponding fresh metadata is available.
 useEffect(()=>{
  if(!canvas.current)return;
  const map=createEngine(canvas.current,{viewport:(bounds,value)=>handlers.current.viewport(bounds,value),pick:(id,multiple)=>handlers.current.pick(id,multiple),ready:setEngineReady,error:()=>setEngineError(true)});
  engine.current=map;
  map.setActiveLayer(initial.configuration.layers[0]?.visible?initial.configuration.layers[0].layer_id:null);
  map.setBasemap(defaultBasemap);map.setCamera(initial.configuration.default_viewpoint??defaultCamera);
  return ()=>{map.clear();map.destroy();engine.current=null;};
 },[]);
 const [prepareViewportFrame]=useState(createViewportFramePreparer);
 const frames:RenderLayer[]=[],frameVersions:number[]=[];
 const states=configuration.layers.map((ref,index)=>{
  const layer=metadata[index],result=viewportQueries[index];
  if(!ref.visible)return "hidden";
  if(!layer)return metadataReads[index]!.status;
  const key=selectionKey({accountId,organizationId,projectId,layerId:layer.id,sourceId:layer.source_id,contentRevision:layer.source.content_revision,schemaRevision:layer.source.schema_revision});
  if(key===blocked||(key===activeKey&&activeBlocked))return "stale — revalidation required";
  if(!capability(layer,"render"))return "unsupported or unavailable";
  if(result?.error)return (result.error instanceof MapApiError&&result.error.code==="STALE_READ_PIN")?"stale":"unavailable";
  const value=result&&currentProtectedData(result);
  if(!value)return !bbox||result?.fetchStatus==="fetching"?"loading":"unavailable";
  const scope={accountId,organizationId,projectId,layerId:layer.id,sourceId:layer.source_id,contentRevision:layer.source.content_revision,schemaRevision:layer.source.schema_revision};
  const prepared=prepareViewportFrame(scope,value,result.dataUpdatedAt);
  if(prepared.status==="invalid-context")return "unavailable — response context mismatch";
  if(prepared.status==="invalid-geometry")return "unavailable — invalid display geometry";
  frames.push({id:layer.id,scopeKey:key,style:ref.style,features:prepared.features});
  frameVersions.push(prepared.version);
  return !value.complete?"partial — "+value.limitation:value.items.length?"usable":"empty";
 });
 const busyData=states.some(s=>s==="loading");
 const frameKey=JSON.stringify([configuration.layers,bbox,frames.map((f,index)=>[f.scopeKey,frameVersions[index]]),metadataQueries.map(q=>[q.dataUpdatedAt,Boolean(q.error)]),viewportQueries.map(q=>[q.dataUpdatedAt,Boolean(q.error)]),blocked,activeBlocked,busyData]);
 useLayoutEffect(()=>{applyCurrentFrameSet(engine.current,frames,busyData);},[frameKey]);
 useLayoutEffect(()=>{engine.current?.setActiveLayer(configuration.layers.some(ref=>ref.layer_id===active&&ref.visible)&&!activeBlocked?active:null);engine.current?.setSelection(activeBlocked?null:active,selected);},[active,activeBlocked,selected.join(","),configuration.layers.map(ref=>ref.layer_id+ref.visible).join(",")]);
 const hasGeometry=frames.some(frame=>frame.features.length>0);
 const state:AccessibleMapState=engineError?"unavailable":busyData?"loading":states.some(s=>s.startsWith("stale"))?"stale":states.some(s=>s.startsWith("partial"))?"partial":hasGeometry?"usable":states.some(s=>s==="permission"||s.startsWith("unsupported")||s.startsWith("unavailable"))?"unavailable":"empty";
 const accessibleSummary=buildAccessibleMapSummary({
  projectName:project.name,
  references:configuration.layers,
  metadata,
  activeLayerId:active,
  selectedFeatureIds:selected,
  focusedFeatureId:focus,
  state
 });
 const ready=engineReady&&!busyData&&!engineError;
 async function revalidate():Promise<void> {setBlocked(null);setSelection(null);engine.current?.clear();setNotice("");await queryClient.invalidateQueries({queryKey:scopePrefix});}
 async function fitLayers(ids:string[]):Promise<void> {
  const generation=++fitGeneration.current;setNotice("Đang kiểm tra phạm vi hiển thị được cấp quyền…");
  try {
   const values:Bounds[]=[];
   //32 references maximum; four bounded in flight.
   for(let start=0;start<ids.length;start+=4){
    const batch=await Promise.all(ids.slice(start,start+4).map(async id=>{
     const layer=metadata.find(l=>l?.id===id);
     if(!layer||!capability(layer,"render"))return null;
     const scope={accountId,organizationId,projectId,layerId:layer.id,sourceId:layer.source_id,contentRevision:layer.source.content_revision,schemaRevision:layer.source.schema_revision};
     const extent=await getMapDisplayExtent(apiClient,organizationId,projectId,id,{content_revision:scope.contentRevision,schema_revision:scope.schemaRevision});
     if(!viewportMatches(scope,extent.context))throw new Error("Mismatched extent");
     return extent.state==="available"&&extent.bounds?extent.bounds as Bounds:null;
    }));for(const value of batch)if(value)values.push(value);
   }
   if(!alive.current||generation!==fitGeneration.current)return;
   const bounds=combineBounds(values);if(bounds){engine.current?.fitBounds(bounds);setNotice("");}else setNotice("Không có phạm vi hiển thị hợp lệ và khác rỗng.");
  }catch(error){if(alive.current&&generation===fitGeneration.current)setNotice(errorMessage(error));}
 }
 async function fitSelection():Promise<void> {
  if(!activeScope||!selected.length)return;
  const token=liveScope.current,generation=++fitGeneration.current,ids=[...selected];setNotice("Đang kiểm tra hình học của các đối tượng đã chọn…");
  try {
   let positions=0,bytes=0;const bounds:(Bounds|null)[]=[];
   for(let start=0;start<ids.length;start+=4){
    const records=await Promise.all(ids.slice(start,start+4).map(id=>getSpatialFeature(apiClient,organizationId,projectId,activeScope.layerId,id,{content_revision:activeScope.contentRevision,schema_revision:activeScope.schemaRevision,include_geometry:true,response_crs:4326})));
    for(const record of records){
     if(!viewportMatches(activeScope,record.context)||!ids.includes(record.feature.feature_id))throw new Error("Selection changed");
     positions+=positionCount(record.feature.geometry?.coordinates);bytes+=new TextEncoder().encode(JSON.stringify(record)).byteLength;
     if(positions>20000||bytes>2097152)throw new Error("Selection extent exceeds the supported geometry budget.");
     bounds.push(boundsOfGeometry(record.feature.geometry));
    }
   }
   if(!alive.current||token!==liveScope.current||generation!==fitGeneration.current)return;
   const value=combineBounds(bounds);if(value){engine.current?.fitBounds(value);setNotice("");}else setNotice("Vùng chọn không có hình học hiển thị hợp lệ và khác rỗng.");
  }catch(error){if(alive.current&&token===liveScope.current){setNotice(error instanceof ApiError?errorMessage(error):"Phạm vi vùng chọn không khả dụng hoặc vượt giới hạn hình học cho phép.");if(requiresRevalidation(error))setBlocked(activeKey);}}
 }
 useEffect(()=>{
  if(initialFit.current||!engine.current||configuration.default_viewpoint||!configuration.layers.length)return;
  const ids=configuration.layers.filter((ref,index)=>ref.visible&&capability(metadata[index],"render")).map(ref=>ref.layer_id);
  if(ids.length){initialFit.current=true;void fitLayers(ids);}
 },[metadataQueries.map(q=>q.dataUpdatedAt).join(","),configuration.layers.length]);
 async function acceptReceipt(receipt:MapSaveReceipt,input:SaveMapInput):Promise<void> {
  if(!receiptMatches({input,status:"saving"},receipt))throw new Error("Unconfirmed save receipt");
  const current=await getMap(apiClient,organizationId,projectId);
  if(!mapResponseMatches(organizationId,projectId,current)||current.map_id!==saved.map_id||current.configuration_revision<receipt.configuration_revision)throw new Error("Unconfirmed persisted revision");
  if(!alive.current)return;
  setSaved(current);setConfiguration(current.configuration);setViewpointIntent({mode:"preserve"});setPending({input,status:"committed"});
  setNotice("Đã lưu phiên bản "+receipt.configuration_revision+". Phiên bản dùng chung hiện tại "+current.configuration_revision+".");
  queryClient.setQueryData([...scopePrefix,"definition"],current);
  await queryClient.invalidateQueries({queryKey:[...scopePrefix,"history"]});
 }
 async function submitSave(retry=false):Promise<void> {
  if(!writable||working)return;
  const input=retry&&pending?pending.input:{operation_id:crypto.randomUUID(),expected_revision:saved.configuration_revision,layers:configuration.layers,basemap:"grid" as const,default_viewpoint:viewpointIntent};
  const operation=beginSave(input);setPending(operation);setWorking(true);setNotice("");
  try{await loadCsrfToken(apiClient);const receipt=await saveMap(apiClient,organizationId,projectId,input);await acceptReceipt(receipt,input);}
  catch(error){if(alive.current){const status=saveErrorState(error instanceof ApiError?error.status:undefined);setPending(status==="unresolved"?markUnresolved(operation):{...operation,status});setNotice(status==="unresolved"?"Kết quả lưu chưa xác định. Hãy đối chiếu hoặc thử lại đúng thao tác này trước khi tạo bản lưu mới.":errorMessage(error));if(status==="denied"){engine.current?.clear();await queryClient.invalidateQueries({queryKey:scopePrefix});}}}
  finally{if(alive.current)setWorking(false);}
 }
 async function reconcile():Promise<void> {
  if(!pending||!writable||working)return;setWorking(true);
  try{const receipt=await getMapOperation(apiClient,organizationId,projectId,pending.input.operation_id);await acceptReceipt(receipt,pending.input);}
  catch(error){if(alive.current){setNotice("Kết quả lưu vẫn chưa xác định. "+errorMessage(error));if(error instanceof ApiError&&[401,403].includes(error.status)){setPending({...pending,status:"denied"});engine.current?.clear();await queryClient.invalidateQueries({queryKey:scopePrefix});}}}
  finally{if(alive.current)setWorking(false);}
 }
 async function reloadShared():Promise<void>{
  if(pending?.status==="unresolved"){setNotice("Hãy đối chiếu kết quả lưu đang chờ trước khi thay thế cấu hình cục bộ.");return;}
  setWorking(true);
  try{const current=await getMap(apiClient,organizationId,projectId);if(!mapResponseMatches(organizationId,projectId,current))throw new Error("Phạm vi dữ liệu không khớp");setSaved(current);setConfiguration(current.configuration);setViewpointIntent({mode:"preserve"});setPending(null);setSelection(null);setActive(current.configuration.layers[0]?.layer_id??null);setNotice("Đã tải phiên bản bản đồ "+current.configuration_revision+".");}
  catch(error){setNotice(errorMessage(error));}finally{setWorking(false);}
 }
 function execute(id:MapCommandId):void {
  setContextMenu(false);
  if(id==="map.add-data")setAddDataOpen(true);
  if(id==="map.fit-layer"&&active)void fitLayers([active]);
  if(id==="map.fit-all")void fitLayers(visibleRenderableIds);
  if(id==="map.fit-selection")void fitSelection();
  if(id==="map.clear-selection")setSelection(null);
  if(id==="map.save")void submitSave();
  if(id==="map.set-default-viewpoint"){const current=engine.current?.getCamera()??camera;setViewpointIntent({mode:"set",value:current});setNotice("Đã đánh dấu góc nhìn hiện tại. Bấm Lưu bản đồ để ghi nhận.");}
  if(id==="map.zoom-in")engine.current?.zoom(1);if(id==="map.zoom-out")engine.current?.zoom(-1);
  if(id==="map.open-projects")void navigate(organizationPath+"/projects");
  if(id==="map.open-catalog")void navigate(projectPath+"/spatial");
  if(id==="map.open-transfers")void navigate(projectPath+"/transfers");
  if(id==="map.open-access"&&canManageAccess)void navigate(organizationPath+"/membership");
  if(id.startsWith("map.open-layer:")){
   const layerId=id.slice("map.open-layer:".length);
   if(metadata.some(layer=>layer?.id===layerId))choose(layerId);
  }
 }
 const visibleMetadataPending=configuration.layers.some((ref,index)=>ref.visible&&metadataReads[index]?.status==="loading");
 const visibleRenderableIds=configuration.layers.filter((ref,index)=>ref.visible&&capability(metadata[index],"render")).map(ref=>ref.layer_id);
 const activeLayerView=activeIndex>=0?buildLayerPanelItemView(configuration.layers[activeIndex]!,activeLayer,states[activeIndex]??"loading",active,activeIndex,configuration.layers.length):undefined;
 const commands=createCommands({hasActive:activeLayerView?.canZoom??false,hasVisible:!visibleMetadataPending&&visibleRenderableIds.length>0,hasSelection:selected.length>0&&capability(activeLayer,"render")&&!activeBlocked,canSave:writable,canAddData:configuration.layers.length<32,busy:locked,navigation:{canManageAccess,layers:metadata.flatMap(layer=>layer?[{id:layer.id,name:layer.name}]:[])}},execute);
 const addDataCommand=commands.find(command=>command.id==="map.add-data");
 const commandRef=useRef(commands);commandRef.current=commands;
 useEffect(()=>{
  const key=(event:KeyboardEvent)=>{if(event.target instanceof HTMLElement&&(event.target.closest("input,textarea,select,[contenteditable=true]")))return;
   let id:string|null=null;if((event.ctrlKey||event.metaKey)&&event.key.toLowerCase()==="s")id="map.save";if(event.key.toLowerCase()==="f")id="map.fit-layer";if(event.key==="Escape")id="map.clear-selection";if(event.key==="+")id="map.zoom-in";if(event.key==="-")id="map.zoom-out";if(id){event.preventDefault();invokeCommand(commandRef.current,id,"shortcut");}};
  window.addEventListener("keydown",key);return()=>window.removeEventListener("keydown",key);
 },[]);
 const invalidPage=Boolean(activeScope&&capability(activeLayer,"feature_read")&&pageQuery.fetchStatus==="idle"&&pageQuery.isSuccess&&!page);
 const invalidDetail=Boolean(activeScope&&focus&&detailQuery.fetchStatus==="idle"&&detailQuery.isSuccess&&!detail);
 const readError=activeBlocked?"Lớp hoặc đối tượng đang chọn cần xác thực lại quyền truy cập hiện hành.":pageQuery.error?errorMessage(pageQuery.error):invalidPage?"Không thể xác nhận dữ liệu bảng thuộc tính thuộc lớp và phiên bản hiện tại.":null;
 const detailError=activeBlocked?readError:detailQuery.error?errorMessage(detailQuery.error):invalidDetail?"Không thể xác nhận chi tiết đối tượng thuộc lớp và phiên bản hiện tại.":null;
 const dirty=JSON.stringify(configuration.layers)!==JSON.stringify(saved.configuration.layers)||viewpointIntent.mode!=="preserve"||saved.configuration_revision===0;
 const saveIndicatorView=buildSaveIndicatorView({
  dirty,
  pendingStatus:pending?.status??null,
  revision:saved.configuration_revision
 });
 const navigationBlocker=useUnsavedNavigationProtection(dirty);
 function change(layers:MapLayerReference[]):void {if(locked)return;setConfiguration({...configuration,layers});setPending(null);if(active&&!layers.some(l=>l.layer_id===active))choose(layers[0]?.layer_id??"");}
 function updateLayoutPreference(value:WorkspaceLayoutPreference):void {
  const normalized=normalizeWorkspaceLayoutPreference(value);
  setLayoutPreference(normalized);
  saveWorkspaceLayoutPreference(normalized);
 }
 const addDataCandidates=buildAddDataCandidates(catalog?.items??[],configuration.layers);
 const technicalDetailsView=buildTechnicalDetailsView({organizationId,projectId,map:saved,activeLayer});
 function addLayerToMap(layerId:string):void {
  const candidate=addDataCandidates.find(item=>item.layerId===layerId);
  if(locked||configuration.layers.length>=32||!candidate?.addable)return;
  change([...configuration.layers,{layer_id:layerId,visible:true,style:{...defaultStyle}}]);
  choose(layerId);
  setAddDataOpen(false);
 }
 return <main className="gis-workspace" data-testid="map-workspace">
  <MapRibbon
   organizationId={organizationId}
   projectName={project.name}
   revision={saved.configuration_revision}
   writable={writable}
   archived={project.status==="archived"}
   dirty={dirty}
   saveState={saveIndicatorView.state}
   saveLabel={saveIndicatorView.label}
   commands={commands}
   activeTool={activeTool}
   activeModules={activeModules}
   leftOpen={!layoutPreference.leftPanelCollapsed}
   rightOpen={!layoutPreference.rightPanelCollapsed}
   tableOpen={!layoutPreference.attributeTableCollapsed}
   onToggleLeft={()=>updateLayoutPreference({...layoutPreference,leftPanelCollapsed:!layoutPreference.leftPanelCollapsed})}
   onToggleRight={()=>updateLayoutPreference({...layoutPreference,rightPanelCollapsed:!layoutPreference.rightPanelCollapsed})}
   onToggleTable={()=>updateLayoutPreference({...layoutPreference,attributeTableCollapsed:!layoutPreference.attributeTableCollapsed})}
   onSelectTool={setActiveTool}
   needsRecovery={pending?.status==="unresolved"||pending?.status==="conflict"}
   sharedControlsOpen={sharedControlsOpen}
   cmdLineOpen={cmdLineOpen}
   onToggleCmdLine={()=>setCmdLineOpen(v=>!v)}
   onOpenTechnical={()=>setTechnicalDetailsOpen(true)}
   onToggleSharedControls={()=>setSharedControlsOpen(open=>!open)}
  />
  <div className="gis-workstation-stage">
   <MapActivityRail
    leftOpen={!layoutPreference.leftPanelCollapsed}
    activeTool={activeTool}
    activeModules={activeModules}
    onToggleLeft={()=>updateLayoutPreference({...layoutPreference,leftPanelCollapsed:!layoutPreference.leftPanelCollapsed})}
    onSelectTool={setActiveTool}
   />
   <div className="gis-workstation-main">
    <WorkspaceLayout
     preference={layoutPreference}
     onPreferenceChange={updateLayoutPreference}
     left={<LayerTree layers={configuration.layers} metadata={metadata} active={active} locked={locked} states={states} choose={choose} change={change}
      zoom={id=>void fitLayers([id])}
      addDataEnabled={addDataCommand?.enabled??false} addDataReason={addDataCommand?.reason??null}
      openAddData={()=>invokeCommand(commands,"map.add-data","toolbar")}/>}
     map={<section className="map-stage" aria-label="Khung bản đồ" aria-describedby="map-accessible-summary" onContextMenu={e=>{e.preventDefault();setContextMenu(!contextMenu);}}>
      <p id="map-accessible-summary" className="sr-only" data-testid="accessible-map-summary" aria-live="polite">{accessibleSummary.text}</p>
      <div ref={canvas} className="map-canvas" data-testid="map-canvas"/>
      <div className="map-engine-signal" data-testid="map-engine-ready" data-ready={ready?"true":"false"} aria-hidden="true"/>
      <div className="map-stage-status" role="status"><strong data-testid="map-status" data-state={state}>{mapStateLabel(state)}</strong><span>{BASEMAP_OPTIONS.find(b=>b.id===basemap)?.label??"Lưới cục bộ"} · EPSG:3857 / EPSG:4326</span></div>
      {state==="empty"?<div className="map-blank-message"><h2>Bản đồ dự án</h2><p>{configuration.layers.length?"Không có hình học có thể đọc trong khung nhìn này.":"Hãy thêm một lớp dữ liệu từ bảng bên trái để bắt đầu."}</p></div>:null}
      {state==="partial"?<div className="map-partial" role="status">Khung nhìn đang hiển thị một phần dữ liệu. Hãy phóng to để xem chi tiết hơn. Bảng thuộc tính đọc độc lập các bản ghi đúng phiên bản.</div>:null}
      <details ref={basemapDetailsRef} className="map-basemap-picker">
       <summary>Nền: {BASEMAP_OPTIONS.find(b=>b.id===basemap)?.label??"Lưới cục bộ"} ▾</summary>
       <div className="map-basemap-menu" role="group" aria-label="Bản đồ nền">
        {BASEMAP_OPTIONS.map(opt=><button key={opt.id} type="button" data-testid={"basemap-"+opt.id} disabled={!opt.available} aria-pressed={basemap===opt.id} title={opt.available?undefined:ONLINE_BASEMAP_UNAVAILABLE} className={basemap===opt.id?"is-selected":""} onClick={(ev)=>{if(isBasemapAvailable(opt.id)){setBasemap(opt.id);ev.currentTarget.closest("details")?.removeAttribute("open");}}}>{opt.label}</button>)}
       </div>
      </details>
      <MapCommandLine
       commands={commands}
       activeModules={activeModules}
       visible={cmdLineOpen}
       onSelectTool={setActiveTool}
       onToggleLeft={()=>updateLayoutPreference({...layoutPreference,leftPanelCollapsed:!layoutPreference.leftPanelCollapsed})}
       onToggleRight={()=>updateLayoutPreference({...layoutPreference,rightPanelCollapsed:!layoutPreference.rightPanelCollapsed})}
       onToggleTable={()=>updateLayoutPreference({...layoutPreference,attributeTableCollapsed:!layoutPreference.attributeTableCollapsed})}
       onOpenTechnical={()=>setTechnicalDetailsOpen(true)}
       onReloadShared={()=>void reloadShared()}
       onHide={()=>setCmdLineOpen(false)}
      />
      {contextMenu?<div className="map-context-menu" role="menu">{commands.filter(isCanvasCommand).map(c=><button role="menuitem" key={c.id} disabled={!c.enabled} title={c.reason??c.label} onClick={()=>invokeCommand(commands,c.id,"context")}>{c.label}</button>)}</div>:null}
     </section>}
     right={<FeatureInspector layer={activeLayer} detail={detail} focus={focus} count={selected.length} error={detailError}/>}
     bottom={<AttributeTable layer={capability(activeLayer,"feature_read")?activeLayer:undefined} page={page} selected={selected} focus={focus} select={select} pageNumber={pages.length+1} error={readError}
      previous={pages.length?()=>{setCursor(pages[pages.length-1]??null);setPages(pages.slice(0,-1));}:null}
      next={page?.next_after_feature_id?()=>{setPages([...pages,cursor]);setCursor(page.next_after_feature_id);}:null}/>}
    />
    {activeTool?(
     <MapToolDock
      tool={activeTool}
      organizationId={organizationId}
      projectId={projectId}
      activeModules={activeModules}
      onClose={()=>setActiveTool(null)}
     />
    ):null}
   </div>
  </div>
  <footer className="gis-statusbar" role="status" aria-label="Thanh trạng thái bản đồ">
   <span className="gis-sb-item is-live">● {mapStateLabel(state)}</span>
   <span className="gis-sb-item">Tọa độ: {formatCoordinateInCrs(camera.longitude,camera.latitude,displayCrs)}</span>
   <span className="gis-sb-item">Zoom: {camera.zoom.toFixed(2)}</span>
   <span className="gis-sb-item">Lớp: {configuration.layers.length}</span>
   <span className="gis-sb-item">Chọn: {selected.length}</span>
   {activeTool?<span className="gis-sb-item is-tool">Công cụ: {activeTool}</span>:null}
   <div className="gis-sb-right">
    <MapCrsPicker
     organizationId={organizationId}
     projectId={projectId}
     selectedCode={displayCrs.code}
     onSelect={(next)=>{setDisplayCrs(next);try{window.localStorage.setItem("ovg-display-crs",String(next.code));}catch{/* storage unavailable */}}}
     onOpenFullCrsDock={()=>setActiveTool("spatial")}
    />
   </div>
  </footer>
  <TechnicalDetailsDrawer
   open={technicalDetailsOpen}
   view={technicalDetailsView}
   onClose={()=>setTechnicalDetailsOpen(false)}
  />
  <AddDataDialog
   open={addDataOpen}
   candidates={addDataCandidates}
   catalogStatus={catalogState.status}
   locked={locked}
   importPath={"/app/organizations/"+organizationId+"/projects/"+projectId+"/transfers"}
   catalogPath={"/app/organizations/"+organizationId+"/projects/"+projectId+"/spatial"}
   nextCatalog={catalog?.next_after_id?()=>{setCatalogPages([...catalogPages,catalogCursor]);setCatalogCursor(catalog.next_after_id);}:null}
   previousCatalog={catalogPages.length?()=>{setCatalogCursor(catalogPages[catalogPages.length-1]??null);setCatalogPages(catalogPages.slice(0,-1));}:null}
   onAdd={addLayerToMap}
   onClose={()=>setAddDataOpen(false)}
  />
  {navigationBlocker.state==="blocked"?<div className="map-unsaved-blocker" role="dialog" aria-modal="true" aria-labelledby="map-unsaved-title">
   <div>
    <h2 id="map-unsaved-title">Có thay đổi bản đồ chưa lưu</h2>
    <p>Rời trang lúc này sẽ bỏ các thay đổi cấu hình bản đồ chưa được lưu.</p>
    <div>
     <button type="button" onClick={()=>navigationBlocker.reset()}>Ở lại</button>
     <button type="button" className="is-danger" onClick={()=>navigationBlocker.proceed()}>Rời đi và bỏ thay đổi</button>
    </div>
   </div>
  </div>:null}
  <div className="map-sessionbar"><span>Giữ Shift hoặc Ctrl khi bấm để chọn nhiều · {selected.length}/100 đang chọn</span><span data-testid="map-camera-summary">Tọa độ tâm {camera.longitude.toFixed(5)}, {camera.latitude.toFixed(5)} · mức thu phóng {camera.zoom.toFixed(2)}</span></div>
  <div id="map-shared-controls" className="map-shared-controls" hidden={!sharedControlsOpen}>
  <section className="map-save-panel" aria-label="Cấu hình bản đồ dùng chung"><div><h2>Cấu hình bản đồ dùng chung</h2><SaveStateIndicator dirty={dirty} pendingStatus={pending?.status??null} revision={saved.configuration_revision}/><p className="map-muted">Góc nhìn mặc định: {viewpointIntent.mode==="preserve"?(saved.configuration.default_viewpoint?"giữ nguyên":"chưa đặt"):viewpointIntent.mode==="clear"?"xóa khi lưu":"cập nhật khi lưu"}</p></div>
   <button disabled={!writable||locked} onClick={()=>invokeCommand(commands,"map.set-default-viewpoint","toolbar")}>Đặt góc nhìn mặc định</button>
   <button disabled={!writable||locked} onClick={()=>{setViewpointIntent({mode:"clear"});setPending(null);}}>Xóa góc nhìn mặc định</button>
   <button disabled={!writable||locked} onClick={()=>void submitSave()}>Lưu bản đồ</button>
   <button disabled={working||pending?.status==="unresolved"} onClick={()=>void reloadShared()}>Tải lại bản đồ chung</button>
   {pending?.status==="unresolved"?<><button disabled={working||!writable} onClick={()=>void reconcile()}>Đối chiếu bản lưu</button><button disabled={working||!writable} onClick={()=>void submitSave(true)}>Thử lưu lại</button></>:null}
   <button onClick={()=>void revalidate()}>Kiểm tra lại quyền truy cập</button>
   <button onClick={()=>setShowHistory(!showHistory)}>{showHistory?"Ẩn":"Xem"} lịch sử bản đồ</button>
  </section>
  {showHistory?<section className="map-history" aria-label="Lịch sử cấu hình bản đồ"><h2>Lịch sử cấu hình bản đồ</h2>{history?<><ol>{history.items.map(revision=><li key={revision.configuration_revision}><strong>Phiên bản {revision.configuration_revision}</strong> · {revision.actor_display_snapshot} · {revision.created_at}<details><summary>Cấu hình trước / sau</summary><pre>{JSON.stringify({before:revision.before_configuration,after:revision.after_configuration},null,2)}</pre></details></li>)}</ol><button disabled={history.next_after_revision===null} onClick={()=>{if(history.next_after_revision!==null)setHistoryCursor(history.next_after_revision);}}>Các phiên bản cũ hơn</button><button disabled={historyCursor===undefined} onClick={()=>setHistoryCursor(undefined)}>Các phiên bản mới nhất</button></>:<p>{historyQuery.error?"Không thể truy cập lịch sử hiện tại.":"Đang kiểm tra lịch sử bản đồ…"}</p>}</section>:null}
  </div>
  {notice?<p className="map-notice" role="status">{notice}</p>:null}
 </main>;
}

function ProtectedMap({organizationId,projectId,accountId}:{organizationId:string;projectId:string;accountId:string}) {
 const prefix=["map-workspace",accountId,organizationId,projectId];
 const projectQuery=useQuery({queryKey:[...prefix,"project"],queryFn:()=>getProject(apiClient,organizationId,projectId),...protectedQueryOptions});
 const permissionQuery=useQuery({queryKey:[...prefix,"permissions"],queryFn:()=>getEffectivePermissions(apiClient,organizationId,{project_id:projectId}),...protectedQueryOptions});
 const mapQuery=useQuery({queryKey:[...prefix,"definition"],queryFn:()=>getMap(apiClient,organizationId,projectId),...protectedQueryOptions});
 // Fetch/parse only static renderer code alongside protected auth/definition reads.
 // No map instance or protected query data is shared by the module loader.
 const [adapter,setAdapter]=useState<Awaited<ReturnType<typeof loadMapEngine>>>();
 const [adapterError,setAdapterError]=useState(false);
 useEffect(()=>{
  let disposed=false;
  void loadMapEngine().then(value=>{if(!disposed)setAdapter(value);},()=>{if(!disposed)setAdapterError(true);});
  return ()=>{disposed=true;};
 },[]);
 const project=currentProtectedData(projectQuery),permissions=currentProtectedData(permissionQuery),map=currentProtectedData(mapQuery);
 if(!project||!permissions||!map||project.id!==projectId||project.organization_id!==organizationId||project.status==="deleted"||!mapResponseMatches(organizationId,projectId,map)||!Array.isArray(map.configuration?.layers)||!Array.isArray(permissions.permissions)||!permissions.permissions.includes("project.read")){
  const gate=blockedMapGateStatus([projectQuery,permissionQuery,mapQuery]);
  return <main className="map-access"><Link to={"/app/organizations/"+organizationId+"/projects"}>← Dự án</Link><h1>Không gian Bản đồ</h1><p role={gate==="loading"?"status":"alert"}>{gate==="error"?"Không có quyền truy cập bản đồ dự án này.":gate==="loading"?"Đang kiểm tra quyền truy cập bản đồ dự án…":"Không thể xác nhận phạm vi dữ liệu và quyền truy cập bản đồ dự án này."}</p></main>;
 }
 if(!adapter)return <main className="map-access"><Link to={"/app/organizations/"+organizationId+"/projects"}>← Dự án</Link><h1>Không gian Bản đồ</h1><p role={adapterError?"alert":"status"}>{adapterError?"Không thể khởi tạo công cụ bản đồ. Hãy tải lại trang.":"Đang khởi tạo công cụ bản đồ…"}</p></main>;
 return <Workspace key={accountId+":"+organizationId+":"+projectId} organizationId={organizationId} projectId={projectId} accountId={accountId} project={project} initial={map} canEdit={permissions.permissions.includes("project.edit")} canManageAccess={permissions.permissions.some(permission=>["organization.manage_members","authorization.manage_grants","invitation.manage"].includes(permission))} createEngine={adapter.createMapLibreEngine}/>;
}
export function MapWorkspace() {
 const {organizationId,projectId}=useParams();
 if(!organizationId||!projectId)return <p role="alert">Thiếu ngữ cảnh dự án.</p>;
 return <FreshAccount key={organizationId+":"+projectId}>{account=><ProtectedMap key={account.id+":"+organizationId+":"+projectId} accountId={account.id} organizationId={organizationId} projectId={projectId}/>}</FreshAccount>;
}
