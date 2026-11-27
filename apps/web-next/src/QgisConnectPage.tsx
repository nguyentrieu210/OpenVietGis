import {useEffect, useState} from "react";
import {ApiError, getCurrentUser, listIdentityProviders, listMyOrganizations, listProjects, listQgisBindingCandidates, listIntegrationBindingDrafts, createQgisBindingDraft, activateQgisBinding, issueBindingPairingToken, loadCsrfToken, type CurrentUser, type IdentityProviderSummary, type MyOrganization, type Project} from "@openvietgis/client-sdk";
import {Brand} from "./Brand";

import {apiClient as client} from "./workbenches/app/api";
const key = "ovg-qgis-browser-login-v1";
type Request = {port:number;state:string;expires:number};
function capture(): Request | null {
  const hash = new URLSearchParams(location.hash.slice(1));
  if (hash.has("port") || hash.has("state")) {
    const port = Number(hash.get("port")), state = hash.get("state") ?? "";
    history.replaceState(null, "", location.pathname + location.search);
    if (!Number.isInteger(port) || port < 1024 || port > 65535 || !/^[A-Za-z0-9_-]{43}$/.test(state)) {
      sessionStorage.removeItem(key);
      return null;
    }
    sessionStorage.setItem(key, JSON.stringify({port,state,expires:Date.now()+300000}));
  }
  try {
    const value = JSON.parse(sessionStorage.getItem(key) ?? "null") as Request | null;
    if (value && Number.isInteger(value.port) && value.port >= 1024 && value.port <= 65535 && /^[A-Za-z0-9_-]{43}$/.test(value.state) && value.expires > Date.now()) return value;
  } catch { /* invalid handoff */ }
  sessionStorage.removeItem(key);
  return null;
}
export function QgisConnectPage() {
  const [request] = useState(capture);
  const [user,setUser] = useState<CurrentUser|null>(null);
  const [providers,setProviders] = useState<IdentityProviderSummary[]>([]);
  const [orgs,setOrgs] = useState<MyOrganization[]>([]), [projects,setProjects] = useState<Project[]>([]);
  const [org,setOrg] = useState(""), [project,setProject] = useState("");
  const [loading,setLoading] = useState(true), [busy,setBusy] = useState(false), [error,setError] = useState("");
  useEffect(()=>{
    if(!request){setLoading(false);return;}
    getCurrentUser(client).then(async who=>{setUser(who);setOrgs(await listMyOrganizations(client));}).catch(async e=>{
      if(e instanceof ApiError && e.status===401){setProviders(await listIdentityProviders(client));}
      else setError("Không thể tải tài khoản hoặc tổ chức.");
    }).catch(()=>setError("Không thể tải phương thức đăng nhập.")).finally(()=>setLoading(false));
  },[request]);
  useEffect(()=>{
    setProject("");setProjects([]);
    if(!org)return;
    let active=true;
    listProjects(client,org).then(items=>{if(active)setProjects(items.filter(p=>p.status==="active"));}).catch(()=>{if(active)setError("Không thể tải dự án.");});
    return()=>{active=false;};
  },[org]);
  async function approve(){
    if(!request || request.expires <= Date.now() || !org || !project || busy)return;
    setBusy(true);setError("");
    try {
      const [candidates,bindings] = await Promise.all([listQgisBindingCandidates(client,org,project),listIntegrationBindingDrafts(client,org,project)]);
      if(!candidates.items.length)throw Error("Dự án chưa có lớp bản đồ hợp lệ.");
      const csrf = await loadCsrfToken(client);
      const layer = candidates.items[0]!;
      let binding = bindings.items.find(b=>b.state==="active") ?? bindings.items.find(b=>b.layer_id===layer.layer_id && b.state!=="revoked");
      if(!binding)binding = await createQgisBindingDraft(client,org,project,{
        layer_id:layer.layer_id,adapter_key:"qgis",external_dataset_ref:"qgis_"+layer.layer_id,
        direction:"bidirectional_manual",source_crs_code:layer.stored_crs_code,target_crs_code:layer.stored_crs_code,
        external_primary_key:"ovg_feature_id",allowed_field_ids:layer.allowed_field_ids
      },csrf);
      if(binding.state!=="active")binding = await activateQgisBinding(client,org,project,binding.id,binding.revision,csrf);
      if(request.expires <= Date.now())throw Error("Phiên kết nối đã hết hạn.");
      const issued = await issueBindingPairingToken(client,org,project,binding.id,{label:"QGIS Desktop",expires_in_days:30},csrf);
      // Explicitly approved top-level form POST to the private one-time loopback listener.
      // Do not place the credential in URL, storage, or browser logs.
      // The browser otherwise sends Origin: null on an HTTPS-to-HTTP form.
      // Send only the site origin (never the URL/fragment or token).
      const policy = document.createElement("meta");
      policy.name="referrer";policy.content="origin";document.head.append(policy);
      const form = document.createElement("form");
      form.method="POST";form.action="http://127.0.0.1:"+request.port+"/callback";
      for(const [name,value] of Object.entries({state:request.state,pairing_code:issued.pairing_code})){
        const input=document.createElement("input");input.type="hidden";input.name=name;input.value=value;form.append(input);
      }
      sessionStorage.removeItem(key);
      document.body.append(form);form.submit();
    }catch(e){
      setError(e instanceof ApiError ? "Không thể cấp quyền (HTTP "+e.status+"). Cần quyền quản trị dự án." : e instanceof Error ? e.message : "Không thể kết nối.");
      setBusy(false);
    }
  }
  return <main className="auth-complete"><Brand/><section className="panel" style={{maxWidth:480,margin:"24px auto",padding:24,textAlign:"left"}}>
    <h1>Kết nối QGIS Desktop</h1>
    {!request?<p>Phiên kết nối không hợp lệ hoặc đã hết hạn. Hãy mở lại từ QGIS.</p>:loading?<p>Đang kiểm tra tài khoản…</p>:!user?<><p>Đăng nhập bằng tài khoản OpenVietGIS. Không nhập mật khẩu trong plugin.</p>{providers.map(provider=><p key={provider.id}><a className="primary" href={"/api/v1/auth/login/"+encodeURIComponent(provider.id)+"?return_to="+encodeURIComponent("/app?qgis_connect=1")+"&prompt=login"}>Đăng nhập với {provider.display_name}</a></p>)}</>:<>
      <p>Chỉ tiếp tục nếu chính bạn vừa bấm đăng nhập trong QGIS trên máy này. Chọn dự án để cấp kết nối; quyền được kiểm tra trên máy chủ.</p>
      <label>Tổ chức<select value={org} disabled={busy} onChange={e=>setOrg(e.target.value)}><option value="">Chọn tổ chức</option>{orgs.filter(x=>!x.status||x.status==="active").map(x=><option key={x.id} value={x.id}>{x.name}</option>)}</select></label>
      <label>Dự án<select value={project} disabled={busy} onChange={e=>setProject(e.target.value)}><option value="">Chọn dự án</option>{projects.map(x=><option key={x.id} value={x.id}>{x.name}</option>)}</select></label>
      <p>Khóa thiết bị có thời hạn 30 ngày, giới hạn trong dự án đã chọn. Có thể thu hồi trong trang quản lý kết nối.</p>
      <button className="primary" disabled={!org||!project||busy} onClick={()=>void approve()}>{busy?"Đang cấp quyền…":"Cấp quyền và kết nối QGIS"}</button>
    </>}
    {error&&<p role="alert" className="error">{error}</p>}
  </section></main>;
}
