import {PluginLicensePanel} from "./PluginLicensePanel";
import {PluginGraphPanel} from "./PluginGraphPanel";
import {DeclarativePluginPanel} from "./DeclarativePluginPanel";
import {activePartner,partnerActivationPin} from "./plugin-lifecycle";
import React, {useCallback, useEffect, useRef, useState} from "react";
import {ApiError, activateProjectExtension, disableProjectExtension, listExtensionManifests, listProjectActivations, getEffectivePermissions, loadCsrfToken, partnerPanel, registerPartnerPlugin, listMarketplace, listMarketplaceV2, installMarketplaceRelease, savePartnerContributions, type MarketplaceRelease, type SavedPartnerContribution, type PartnerContributionDocument, type ApiClient, type ExtensionManifest, type ExtensionActivation, type PartnerCapability} from "@openvietgis/client-sdk";
import {navigateTarget,partnerOverlay,partnerTool,type NavigateTarget,type PartnerMapBridge,type PartnerMapMethod,type PartnerOverlay,type PartnerToolInput,type PartnerTool} from "./partner-map-bridge";
import "./external-plugin-hub.css";

const protocol = "openvietgis.partner.v1";
const errorText = (e: unknown) => e instanceof ApiError ? "API HTTP " + e.status : e instanceof Error ? e.message : "Không thể thực hiện thao tác";
const labels: Record<PartnerCapability,string> = {host_context:"Đọc ngôn ngữ và phiên bản SDK",host_notice:"Hiển thị thông báo trong khung plugin",map_view:"Yêu cầu chia sẻ vùng bản đồ (WGS84)",map_selection:"Yêu cầu chia sẻ hình học đối tượng đang chọn",map_navigate:"Yêu cầu di chuyển bản đồ",map_overlay:"Thêm lớp tạm thời vào bản đồ",tool_register:"Thêm nút công cụ vào GIS Studio"};
type MapRequest={method:PartnerMapMethod;target:NavigateTarget|null;overlay:PartnerOverlay|null;tool:PartnerToolInput|null;port:MessagePort;timer:number};
const mapMethods:PartnerMapMethod[]=["map_view","map_selection","map_navigate","map_overlay","tool_register"];

export function ExternalPluginFrame({manifest, panel, bridge, authorize,onToolsChange,saved,save}: {manifest:ExtensionManifest; panel:NonNullable<ReturnType<typeof partnerPanel>>; bridge:React.RefObject<PartnerMapBridge|null>; authorize:(method:PartnerMapMethod)=>Promise<boolean>;onToolsChange:(tools:PartnerTool[])=>void;saved?:SavedPartnerContribution;save?:(document:PartnerContributionDocument)=>Promise<void>}) {
  const frame = useRef<HTMLIFrameElement>(null);
  const [ready,setReady] = useState(false), [notice,setNotice] = useState("");
  const session = useRef(""), lastNotice = useRef(0), mounted=useRef(false);
  const latest=useRef({authorize,onToolsChange,save,saved});latest.current={authorize,onToolsChange,save,saved};
  const tools=useRef<PartnerToolInput[]>([]), toolGeneration=useRef(0);
  const [overlayTitle,setOverlayTitle]=useState(""),[toolCount,setToolCount]=useState(0),[storageStatus,setStorageStatus]=useState(""),[saving,setSaving]=useState(false);
  const overlayRef=useRef<PartnerOverlay|null>(null);
  const clearContributions=(render=true)=>{
    toolGeneration.current++;tools.current=[];overlayRef.current=null;latest.current.onToolsChange([]);bridge.current?.clearOverlay();
    if(render){setOverlayTitle("");setToolCount(0)}
  };
  const registerTool=(tool:PartnerToolInput)=>{
    if(tools.current.length>=4&&!tools.current.some(t=>t.id===tool.id))throw Error("Tối đa 4 công cụ.");
    tools.current=[...tools.current.filter(t=>t.id!==tool.id),tool];
    const generation=++toolGeneration.current,original=session.current;
    latest.current.onToolsChange(tools.current.map(t=>({...t,run:async()=>{
      if(!mounted.current||!original||session.current!==original||generation!==toolGeneration.current)return;
      let allowed=false;try{allowed=await latest.current.authorize("tool_register")}catch{}
      if(!mounted.current||generation!==toolGeneration.current||session.current!==original)return;
      if(!allowed){clearContributions();setNotice("Công cụ không còn quyền thực thi.");return}
      frame.current?.contentWindow?.postMessage({protocol,type:"tool_click",session:original,toolId:t.id},"*");
    }})));
    setToolCount(tools.current.length);
  };
  const pending=useRef<MapRequest|null>(null),lastMapRequest=useRef(0);
  const [request,setRequest]=useState<MapRequest|null>(null),[checking,setChecking]=useState(false);
  const finish=(req:MapRequest,ok:boolean,result:unknown,render=true)=>{
    if(pending.current!==req)return;
    pending.current=null;clearTimeout(req.timer);
    try{req.port.postMessage({protocol,type:"map_response",ok,result})}catch{/* port closed by frame */}finally{req.port.close()}
    if(render){setRequest(null);setChecking(false)}
  };
  const cancel=(reason:string,render=true)=>{if(pending.current)finish(pending.current,false,reason,render)};
  const reset = () => {cancel("Plugin frame changed");clearContributions();session.current=crypto.randomUUID();setReady(false);setNotice("");lastNotice.current=0;frame.current?.contentWindow?.postMessage({protocol,type:"reset"},"*")};
  useEffect(() => {
    mounted.current=true;
    const receive = (event:MessageEvent) => {
      if (!frame.current || event.source !== frame.current.contentWindow || event.origin !== "null") return;
      const data = event.data;
      if (!data || typeof data !== "object" || data.protocol !== protocol) return;
      const send = (response:object) => frame.current?.contentWindow?.postMessage({protocol,...response},"*");
      if (data.type === "hello") {
        if (!session.current) session.current=crypto.randomUUID();
        setReady(true);
        send({type:"welcome",session:session.current,capabilities:panel.capabilities});
        return;
      }
      if (data.type !== "request" || !session.current || data.session !== session.current || typeof data.id !== "string" || !/^[a-zA-Z0-9_-]{1,80}$/.test(data.id)) return;
      const respond = (ok:boolean,result:unknown) => send({type:"response",session:session.current,id:data.id,ok,result});
      if (mapMethods.includes(data.method)) {
        const port=event.ports[0];
        if(!port)return;
        const reject=(reason:string)=>{port.postMessage({protocol,type:"map_response",ok:false,result:reason});port.close()};
        const target=data.method==="map_navigate"?navigateTarget(data.params):null;
        const overlay=data.method==="map_overlay"?partnerOverlay(data.params):null;
        const tool=data.method==="tool_register"?partnerTool(data.params):null;
        if(!panel.capabilities.includes(data.method)||!bridge.current||(data.method==="map_navigate"&&!target)||(data.method==="map_overlay"&&!overlay)||(data.method==="tool_register"&&!tool)){reject("Capability denied or invalid target");return}
        if(pending.current||Date.now()-lastMapRequest.current<2000){reject("Another request is pending or rate limited");return}
        lastMapRequest.current=Date.now();
        const req:MapRequest={method:data.method,target,overlay,tool,port,timer:0};
        req.timer=window.setTimeout(()=>finish(req,false,"Approval expired"),30000);
        pending.current=req;setRequest(req);return;
      }
      if (data.method === "host_context" && panel.capabilities.includes("host_context")) {
        respond(true,{locale:"vi-VN",sdkVersion:"1.0"});
      } else if (data.method === "host_notice" && panel.capabilities.includes("host_notice") && typeof data.params?.text === "string" && data.params.text.length <= 240 && Date.now()-lastNotice.current >= 1500) {
        lastNotice.current=Date.now();setNotice(data.params.text);respond(true,null);
      } else {respond(false,"Capability denied or invalid request");}
    };
    window.addEventListener("message",receive);
    return () => {mounted.current=false;window.removeEventListener("message",receive);session.current="";cancel("Plugin closed",false);clearContributions(false)};
  },[panel.url,panel.capabilities.join(",")]);
  const approvalFlight=useRef<MapRequest|null>(null);
  const approve=async(req:MapRequest)=>{
    if(approvalFlight.current||pending.current!==req)return;
    approvalFlight.current=req;const original=session.current;setChecking(true);
    try {
      const allowed=await latest.current.authorize(req.method);
      if(!mounted.current||session.current!==original||pending.current!==req)return;
      if(!allowed){clearContributions();throw Error("Quyền đã thay đổi hoặc plugin không còn hoạt động.");}
      const host=bridge.current;if(!host)throw Error("Bản đồ không sẵn sàng.");
      let result:unknown=null;
      if(req.method==="map_view")result=host.view();
      else if(req.method==="map_selection")result=host.selection();
      else if(req.method==="map_navigate")host.navigate(req.target!);
      else if(req.method==="map_overlay"){host.overlay(req.overlay!);overlayRef.current=req.overlay!;setOverlayTitle(req.overlay!.title)}
      else if(req.method==="tool_register")registerTool(req.tool!);
      finish(req,true,result??null);
    }catch(e){finish(req,false,errorText(e))}finally{if(approvalFlight.current===req)approvalFlight.current=null}
  };
  const persist=async(clear=false)=>{
    if(!latest.current.save||saving)return;const saveCurrent=latest.current.save,original=session.current;setSaving(true);setStorageStatus("");
    try{await saveCurrent(clear?{overlay:null,tools:[]}:{overlay:overlayRef.current,tools:tools.current});if(!mounted.current||!original||session.current!==original)return;bridge.current?.clearOverlay();setOverlayTitle("");setStorageStatus(clear?"Đã gỡ đóng góp đã lưu.":"Đã lưu đóng góp cho dự án.")}
    catch(e){if(mounted.current&&session.current===original)setStorageStatus(errorText(e))}finally{if(mounted.current)setSaving(false)}
  };
  const restore=async()=>{
    if(!saved||!ready)return;const original=session.current;setStorageStatus("");
    try {
      const overlay=partnerOverlay(saved.document.overlay);
      const entries=saved.document.tools.map(partnerTool).filter((x):x is PartnerToolInput=>!!x);
      if((overlay&&!await latest.current.authorize("map_overlay"))||(entries.length&&!await latest.current.authorize("tool_register")))throw Error("Quyền không còn khả dụng.");
      if(!mounted.current||!original||session.current!==original)return;
      overlayRef.current=overlay;setOverlayTitle("");toolGeneration.current++;tools.current=[];setToolCount(0);onToolsChange([]);entries.forEach(registerTool);setStorageStatus("Đã khôi phục công cụ đã lưu.");
    }catch(e){setStorageStatus(errorText(e))}
  };
  return <div className="partner-frame"><div className="partner-frame-info"><span>{ready?"SDK đã kết nối":"Đang chờ SDK kết nối"} · {new URL(panel.url).hostname}</span><span>Không chia sẻ phiên đăng nhập · dữ liệu GIS chỉ khi bạn xác nhận</span></div>
    <iframe ref={frame} onLoad={reset} src={panel.url} title={"Plugin đối tác: "+manifest.display_name} sandbox="allow-scripts" referrerPolicy="no-referrer" allow="camera 'none'; microphone 'none'; geolocation 'none'; clipboard-read 'none'; clipboard-write 'none'; payment 'none'; fullscreen 'none'"/>
    {request&&<div className="partner-consent" role="region" aria-label="Yêu cầu quyền GIS"><b>{manifest.display_name} · {labels[request.method]}</b><p>{request.method==="map_selection"?"Chỉ gửi hình học 2D của một đối tượng đang chọn (không gửi mã, tên lớp hoặc thuộc tính).":request.method==="map_view"?"Gửi tâm, phạm vi và mức zoom của bản đồ hiện tại.":request.method==="map_overlay"?`Hiển thị ${request.overlay!.geojson.features.length} đối tượng trong lớp tạm “${request.overlay!.title}”. Không ghi vào dự án.`:request.method==="tool_register"?`Thêm nút “${request.tool!.label}” trên thanh công cụ; nút chỉ gửi sự kiện về plugin.`:`Di chuyển đến ${request.target!.longitude}, ${request.target!.latitude} · zoom ${request.target!.zoom}.`}</p><p>Chỉ áp dụng một lần. Đối tác có thể lưu dữ liệu được chia sẻ.</p><button className="primary" disabled={checking} onClick={()=>void approve(request)}>{checking?"Đang kiểm tra quyền…":"Đồng ý một lần"}</button><button onClick={()=>finish(request,false,"User declined")}>Từ chối</button></div>}
    {(overlayTitle||toolCount>0)&&<div className="partner-contributions"><b>Đóng góp của plugin</b>{overlayTitle&&<p>Lớp tạm: {overlayTitle} <button onClick={()=>{bridge.current?.clearOverlay();overlayRef.current=null;setOverlayTitle("")}}>Gỡ lớp</button></p>}{toolCount>0&&<p>{toolCount} nút công cụ <button onClick={()=>{toolGeneration.current++;tools.current=[];setToolCount(0);onToolsChange([])}}>Gỡ công cụ</button></p>}</div>}
    <div className="partner-contributions"><b>Đóng góp lưu trong dự án</b>{saved&&<p>Phiên bản {saved.revision} · {saved.document.tools.length} công cụ {saved.document.overlay?"· lớp bản đồ đã lưu":""}</p>}{saved&&<button disabled={!ready||saving} onClick={()=>void restore()}>Khôi phục công cụ đã lưu</button>}{save&&<><button className="primary" disabled={saving||!ready||(!overlayRef.current&&!toolCount)} onClick={()=>void persist()}>Lưu / thay thế đóng góp cho dự án</button>{saved&&<button disabled={saving} onClick={()=>void persist(true)}>Gỡ đóng góp đã lưu</button>}</>}{storageStatus&&<p role="status">{storageStatus}</p>}</div>
    {notice&&<p role="status" className="partner-notice">{manifest.display_name}: {notice}</p>}
  </div>;
}

export function ExternalPluginHub({client,org,projectId,canRegister,canManage,restricted,expire,onChange,bridge,onDockChange,onToolsChange,savedContributions}:{
  client:ApiClient;org:string;projectId:string;canRegister:boolean;canManage:boolean;restricted:boolean;expire:()=>void;onChange:()=>Promise<void>;bridge:React.RefObject<PartnerMapBridge|null>;onDockChange:(open:boolean)=>void;onToolsChange:(tools:PartnerTool[])=>void;savedContributions:SavedPartnerContribution[];
}) {
  const [items,setItems]=useState<ExtensionManifest[]>([]), [activations,setActivations]=useState<ExtensionActivation[]>([]);
  const [market,setMarket]=useState<MarketplaceRelease[]>([]);
  const [open,setOpen]=useState(""), [error,setError]=useState(""), [busy,setBusy]=useState(false), [loading,setLoading]=useState(true);
  const [register,setRegister]=useState(false),[name,setName]=useState(""),[key,setKey]=useState(""),[version,setVersion]=useState("1.0.0"),[url,setUrl]=useState(""),[description,setDescription]=useState(""),[caps,setCaps]=useState<PartnerCapability[]>(["host_context"]);
  const generation=useRef(0),alive=useRef(true),mutation=useRef(false),openIntent=useRef(0);
  useEffect(()=>{alive.current=true;return()=>{alive.current=false;++generation.current;++openIntent.current}},[]);
  const refresh=useCallback(async()=>{
    const current=++generation.current;
    try {
      const [m,a,p,access]=await Promise.all([listExtensionManifests(client,org),listProjectActivations(client,org,projectId),getEffectivePermissions(client,org,{project_id:projectId}),client.request<{restricted:boolean}>("/organizations/"+org+"/projects/"+projectId+"/access/mine")]);
      if(current!==generation.current)return;
      const valid=access.restricted===false&&p.permissions.includes("project.read")?m.items.filter(x=>partnerPanel(x)):[];
      setItems(valid);setActivations(a.items);
      setOpen(id=>valid.some(x=>x.id===id&&activePartner(x,a.items))?id:"");
      setError("");return {items:valid,activations:a.items};
    } catch(e) {
      if(current===generation.current){setItems([]);setActivations([]);setOpen("");setError(errorText(e));if(e instanceof ApiError&&e.status===401)expire()}
    } finally {if(current===generation.current)setLoading(false)}
  },[client,org,projectId,expire]);
  useEffect(()=>{let live=true;void listMarketplace(client).then(r=>{if(live)setMarket(r.items)}).catch(()=>{if(live)setMarket([])});return()=>{live=false}},[client]);
  useEffect(()=>{void refresh();const timer=window.setInterval(()=>void refresh(),15000);return()=>{generation.current++;clearInterval(timer)}},[refresh]);
  const execute=async(action:()=>Promise<unknown>)=>{if(mutation.current||restricted||!alive.current)return;mutation.current=true;++openIntent.current;setBusy(true);setError("");try{await action();if(!alive.current)return;await refresh();if(alive.current)await onChange()}catch(e){if(alive.current){setError(errorText(e));if(e instanceof ApiError&&e.status===401)expire()}}finally{mutation.current=false;if(alive.current)setBusy(false)}};
  const current=items.find(m=>m.id===open&&activePartner(m,activations)),panel=current?partnerPanel(current):null;
  const docked=!!current&&!!panel&&!restricted;
  useEffect(()=>{onDockChange(docked);return()=>onDockChange(false)},[docked,onDockChange]);
  const [marketQuery,setMarketQuery]=useState("");
  const [declarative,setDeclarative]=useState("");
  const [graph,setGraph]=useState<{root:string;version:string}|null>(null);
  const [v2Eligible,setV2Eligible]=useState<string[]>([]);
  const [licensed,setLicensed]=useState<Record<string,string>>({}),[license,setLicense]=useState("");
  useEffect(()=>{let live=true;const refreshV2=()=>void listMarketplaceV2(client,"web","1.0.0").then(r=>{if(live){setV2Eligible(r.web_read_only_enabled?r.items.map(x=>x.envelope.extension_key+":"+x.envelope.version):[]);setLicensed(Object.fromEntries(r.items.filter(x=>x.license_required).map(x=>[x.envelope.extension_key+":"+x.envelope.version,x.release_digest])))}}).catch(()=>{if(live){setV2Eligible([]);setLicensed({});setLicense("")}});refreshV2();const timer=window.setInterval(refreshV2,15000);return()=>{live=false;clearInterval(timer)}},[client]);
  const selectedV2=items.find(m=>m.id===declarative&&activePartner(m,activations)&&v2Eligible.includes(m.extension_key+":"+m.version));
  useEffect(()=>{setDeclarative("");setGraph(null);setLicense("")},[org,projectId]);
  const marketTerms=marketQuery.toLocaleLowerCase("vi").trim().split(/\s+/).filter(Boolean);
  const visibleMarket=market.filter(r=>{const fields=[r.publisher_id,r.publisher_name,r.manifest.extension_key,r.manifest.display_name,r.manifest.description,r.manifest.version].map(x=>x.toLocaleLowerCase("vi"));return marketTerms.every(term=>fields.some(x=>x.includes(term)))});
  if(restricted)return <section className="partner-hub"><h3>Kho plugin đối tác</h3><p className="live-note">Không mở plugin bên ngoài trong dự án có giới hạn truy cập.</p></section>;
  return <section className="partner-hub"><div className="partner-heading"><div><h3>Kho plugin đối tác</h3><p className="live-note">Danh mục riêng của tổ chức · HTTPS · chạy trong khung cô lập.</p></div>{canRegister&&<button onClick={()=>setRegister(x=>!x)} disabled={busy}>{register?"Đóng biểu mẫu":"Đăng ký plugin"}</button>}</div>
    {register&&canRegister&&<form className="partner-form" onSubmit={e=>{e.preventDefault();void execute(async()=>{await registerPartnerPlugin(client,org,{extension_key:"partner_"+key,version,display_name:name,description,url,capabilities:caps},await loadCsrfToken(client));setRegister(false);setName("");setKey("");setUrl("");setDescription("")})}}>
      <label>Tên plugin<input required maxLength={200} value={name} onChange={e=>setName(e.target.value)}/></label><label>Mã plugin (partner_)<input required pattern="[a-z0-9_]+(?:-[a-z0-9_]+)*" maxLength={92} value={key} onChange={e=>setKey(e.target.value)}/></label>
      <label>Phiên bản<input required pattern="[0-9]+[.][0-9]+[.][0-9]+" value={version} onChange={e=>setVersion(e.target.value)}/></label><label>URL HTTPS phiên bản đã duyệt<input required type="url" value={url} onChange={e=>setUrl(e.target.value)} placeholder="https://plugins.example.com/v1/index.html"/></label>
      <label>Mô tả<input maxLength={500} value={description} onChange={e=>setDescription(e.target.value)}/></label>
      <fieldset><legend>Quyền SDK</legend>{(Object.keys(labels) as PartnerCapability[]).map(c=><label key={c}><input type="checkbox" checked={caps.includes(c)} onChange={e=>setCaps(old=>e.target.checked?[...old,c]:old.filter(x=>x!==c))}/>{labels[c]}</label>)}</fieldset>
      <button className="primary" disabled={busy||!caps.length}>Lưu vào danh mục tổ chức</button>
      <p className="live-note">Chỉ đăng ký URL đã kiểm tra của đối tác tin cậy. Phiên bản và quyền được cố định trong manifest; nội dung do đối tác lưu trữ.</p>
    </form>}
    <section className="partner-marketplace"><h4>Marketplace · plugin đã duyệt</h4><label>Tìm plugin<input type="search" maxLength={200} value={marketQuery} onChange={e=>setMarketQuery(e.target.value)} placeholder="Tên, nhà phát hành, phiên bản"/></label><p className="live-note" aria-live="polite">{visibleMarket.length}/{market.length} plugin</p>{market.length>0&&!visibleMarket.length&&<p>Không tìm thấy plugin phù hợp.</p>}{!market.length&&<p className="live-note">Chưa có bản phát hành công khai đã ký.</p>}{visibleMarket.map(r=><div className="studio-module-card" key={r.manifest.extension_key+":"+r.manifest.version}><div><b>{r.manifest.display_name}</b><small>{r.publisher_name} · v{r.manifest.version} · Đã duyệt chữ ký</small><small>{r.manifest.description}</small></div><button disabled={busy||!canRegister||items.some(m=>m.extension_key===r.manifest.extension_key&&m.version===r.manifest.version)} onClick={()=>void execute(async()=>installMarketplaceRelease(client,org,r,await loadCsrfToken(client)))}>Cài vào tổ chức</button>{(canRegister||canManage)&&v2Eligible.includes(r.manifest.extension_key+":"+r.manifest.version)&&<button disabled={busy} onClick={()=>setGraph({root:r.manifest.extension_key,version:r.manifest.version})}>Đồ thị v2</button>}{canRegister&&licensed[r.manifest.extension_key+":"+r.manifest.version]&&<button disabled={busy} onClick={()=>setLicense(licensed[r.manifest.extension_key+":"+r.manifest.version]!)}>Bản quyền V2</button>}</div>)}</section>
    {license&&canRegister&&Object.values(licensed).includes(license)&&<PluginLicensePanel key={org+":"+license} client={client} org={org} digest={license} onClose={()=>setLicense("")}/>}
    {graph&&(canRegister||canManage)&&<PluginGraphPanel key={org+":"+projectId+":"+graph.root+":"+graph.version} client={client} org={org} project={projectId} selection={graph} canRegister={canRegister} canManage={canManage} onClose={()=>setGraph(null)} onChange={async()=>{await refresh();await onChange()}}/>}
    {loading&&<p>Đang tải danh mục…</p>}{!loading&&!items.length&&!error&&<p className="live-note">Chưa có plugin đối tác được đăng ký.</p>}
    {items.map(m=>{const p=partnerPanel(m)!;const a=activations.find(x=>x.extension_key===m.extension_key),active=activePartner(m,activations),different=a&&a.manifest_id!==m.id;return <div className="studio-module-card" key={m.id}><div><b>{m.display_name}</b><small>{m.extension_key} · v{m.version} · {new URL(p.url).hostname}</small><small>{m.description} · {p.capabilities.map(c=>labels[c]).join("; ")}</small></div>
      {active?<><button disabled={busy} onClick={()=>{const intent=++openIntent.current;setDeclarative("");if(open===m.id){setOpen("");return}void refresh().then(r=>{if(alive.current&&intent===openIntent.current&&r?.items.some(x=>x.id===m.id&&activePartner(x,r.activations)))setOpen(m.id)})}}>{open===m.id?"Đóng khung":"Mở plugin"}</button>{v2Eligible.includes(m.extension_key+":"+m.version)&&<button disabled={busy} onClick={()=>{++openIntent.current;setOpen("");setDeclarative(id=>id===m.id?"":m.id)}}>Mở v2 thử nghiệm</button>}<button disabled={busy||!canManage} onClick={()=>{++openIntent.current;setOpen("");setDeclarative("");void execute(async()=>disableProjectExtension(client,org,projectId,m.extension_key,await loadCsrfToken(client)))}}>Tắt</button></>:<button className="primary" disabled={busy||!canManage||!!different} onClick={()=>void execute(async()=>activateProjectExtension(client,org,projectId,m.id,await loadCsrfToken(client)))}>{different?"Cần chuyển phiên bản":"Kích hoạt"}</button>}</div>})}
    {selectedV2&&!current&&<DeclarativePluginPanel key={org+":"+projectId+":"+partnerActivationPin(selectedV2,activations)} client={client} org={org} project={projectId} manifest={selectedV2.id} canManage={canManage} onClose={()=>setDeclarative("")}/>}
    {current&&panel&&<ExternalPluginFrame key={partnerActivationPin(current,activations)} manifest={current} panel={panel} bridge={bridge} onToolsChange={onToolsChange} saved={savedContributions.find(x=>x.manifest_id===current.id)} save={canManage?async document=>{await savePartnerContributions(client,org,projectId,current.id,savedContributions.find(x=>x.manifest_id===current.id)?.revision??0,document,await loadCsrfToken(client));await onChange()}:undefined} authorize={async method=>{try{const [a,m,p,access]=await Promise.all([listProjectActivations(client,org,projectId),listExtensionManifests(client,org),getEffectivePermissions(client,org,{project_id:projectId}),client.request<{restricted:boolean}>("/organizations/"+org+"/projects/"+projectId+"/access/mine")]);return alive.current&&access.restricted===false&&p.permissions.includes("project.read")&&m.items.some(x=>x.id===current.id&&partnerActivationPin(x,a.items)===partnerActivationPin(current,activations)&&activePartner(x,a.items,method))}catch(e){if(e instanceof ApiError&&e.status===401)expire();return false}}}/> }
    {error&&<p className="error" role="alert">{error}</p>}
  </section>;
}
