import React,{useEffect,useRef,useState} from "react";
import {ApiClient,getCurrentUser} from "@openvietgis/client-sdk";
import {Brand} from "./Brand";
import "./landing.css";
import {LandingSwarm} from "./LandingSwarm";

const client=new ApiClient({baseUrl:"/api/v1"});
export const appPath="/app";
const loginUrl=(provider:string)=>"/api/v1/auth/login/"+encodeURIComponent(provider)+"?return_to="+encodeURIComponent("/app?auth_complete=1")+"&prompt=login";
type Mode="login"|"register";

type MapLayer="boundary"|"road"|"point";
function MapPreview(){
 const [layers,setLayers]=useState<Record<MapLayer,boolean>>({boundary:true,road:true,point:true});
 const [selected,setSelected]=useState(true);
 const toggle=(layer:MapLayer)=>setLayers(v=>({...v,[layer]:!v[layer]}));
 const entries:[MapLayer,string,string][]=[["boundary","Ranh giới","#5ce0c5"],["road","Tuyến đường","#f7c86d"],["point","Điểm dữ liệu","#62aaff"]];
 const points=Array.from({length:77},(_,i)=>{const x=214+(i*73%315),y=68+(i*47%254);return{x,y}});
 return <div className="map-showcase">
 <div className="showcase-halo" aria-hidden="true"/>
 <div className="showcase-window">
  <div className="showcase-header"><div className="showcase-brand"><img src={import.meta.env.BASE_URL+"brand/ovg-mark.png"} alt="" width="28" height="28"/><span><b>OpenVietGIS</b><small>Workspace</small></span></div><span className="showcase-project"><i/> Dự án không gian</span><span className="showcase-avatar">OV</span></div>
  <div className="showcase-body">
   <aside className="showcase-sidebar"><div className="showcase-aside-title"><span>LỚP DỮ LIỆU</span><b>＋</b></div><div className="showcase-group"><span>⌄</span> Dữ liệu khảo sát</div>{entries.map(([layer,label,color])=><button className={"showcase-layer "+(!layers[layer]?"off":"")} key={layer} aria-pressed={layers[layer]} aria-label={"Hiển thị "+label} onClick={()=>toggle(layer)}><span className="showcase-check">{layers[layer]?"✓":""}</span><i style={{background:color}}/>{label}</button>)}<div className="showcase-basemap"><span>◉</span><span>Bản đồ nền<small>WGS 84</small></span></div><span className="showcase-aside-footer">Bấm layer để trải nghiệm</span></aside>
   <div className="showcase-map">
    <svg className="showcase-svg" viewBox="0 0 600 430" preserveAspectRatio="xMidYMid slice" role="img" aria-label="Bản đồ minh họa với ranh giới, đường giao thông và điểm khảo sát">
     <defs><pattern id="overview-grid" width="27" height="27" patternUnits="userSpaceOnUse"><path d="M0 27V0H27" fill="none" stroke="#29444c" strokeWidth=".55"/></pattern><pattern id="overview-parcels" width="42" height="27" patternUnits="userSpaceOnUse" patternTransform="rotate(-18)"><rect x="3" y="3" width="15" height="21" rx="1" fill="#29434a" stroke="#41606a" strokeWidth=".45"/><rect x="21" y="3" width="18" height="21" rx="1" fill="#273f47" stroke="#41606a" strokeWidth=".45"/></pattern><linearGradient id="mapfade" x1="0" x2="1" y1="0" y2="1"><stop offset="0" stopColor="#182d39"/><stop offset="1" stopColor="#1b363d"/></linearGradient><clipPath id="city-clip"><path d="M0 0H600V430H0Z"/></clipPath></defs>
     <rect width="600" height="430" fill="url(#mapfade)"/><rect width="600" height="430" fill="url(#overview-grid)"/>
     <g clipPath="url(#city-clip)" opacity=".9"><path d="M0 0H470L560 80 540 198 600 320V430H90L0 313Z" fill="url(#overview-parcels)"/><path d="M0 75C107 32 181 47 230 86S277 166 353 166 490 56 650 92" stroke="#102b38" strokeWidth="55" fill="none"/><path d="M0 75C107 32 181 47 230 86S277 166 353 166 490 56 650 92" stroke="#27546a" strokeWidth="40" fill="none"/><path d="M0 305C100 340 167 285 205 286S265 343 306 455" stroke="#19333e" strokeWidth="18" fill="none"/><path d="M0 305C100 340 167 285 205 286S265 343 306 455" stroke="#274c5b" strokeWidth="11" fill="none"/>
      <g fill="none" stroke="#4c5f60" strokeWidth="5"><path d="M65 0L119 102 181 151 252 236 319 327 340 430"/><path d="M0 210L117 153 226 191 319 230 480 196 600 218"/><path d="M0 365L161 312 226 191 300 94 322 0"/><path d="M381 0L423 107 401 226 475 329 586 430"/><path d="M0 115L117 153 186 121 300 94 423 107 600 145"/><path d="M0 260L105 243 239 276 388 311 600 296"/><path d="M160 0L181 151 105 243 110 430"/><path d="M600 55L512 125 480 196 475 329 600 374"/></g>
      <g fill="none" stroke="#70827c" strokeWidth="1.5" opacity=".65"><path d="M30 0L77 108 133 225 196 320 230 430"/><path d="M0 175L145 112 249 131 351 218 438 271 600 260"/><path d="M260 0L259 105 298 181 367 254 402 430"/><path d="M0 392L154 374 270 349 426 373 600 349"/></g></g>
     {layers.boundary&&<g data-swarm-layer="boundary" stroke="#5ce0c5" strokeWidth="2" strokeLinejoin="round"><path d="M172 90L260 60 327 104 315 171 239 207 181 165Z" fill="#5ce0c523"/><path d="M327 104L423 94 502 146 479 224 388 254 315 171Z" fill="#53b2d92c" stroke="#6fb8e8"/><path d="M239 207L315 171 388 254 352 336 240 344 187 274Z" fill="#36bba52e"/><path d="M388 254L479 224 533 288 508 357 423 380 352 336Z" fill="#d6b76a2b" stroke="#dbb96c"/><path d="M172 90L260 60 327 104 423 94 502 146 479 224 533 288 508 357 423 380 352 336 240 344 187 274 181 165Z" fill="none" stroke="#71edca" strokeWidth="3"/></g>}
     {layers.road&&<g data-swarm-layer="road" fill="none" strokeLinecap="round" strokeLinejoin="round"><path d="M136 155L237 209 315 171 424 110 502 145" stroke="#e8ba62" strokeWidth="4.5"/><path d="M237 209L254 277 353 333 423 380" stroke="#e8ba62" strokeWidth="4"/><path d="M315 171L388 254 479 224" stroke="#f7c86d" strokeWidth="3.4" strokeDasharray="5 5"/></g>}
     {layers.point&&<g data-swarm-layer="point" fill="#67b6ff" stroke="#193e53" strokeWidth="1.1">{points.filter(p=>p.x<515&&p.y>78&&p.y<330).map((p,i)=><circle key={i} cx={p.x} cy={p.y} r={i%7===0?4.1:2.6} opacity={.68+i%3*.13}/>)}</g>}
     {layers.point&&<g onClick={()=>setSelected(v=>!v)} role="button" tabIndex={0} aria-label="Chọn điểm minh họa" onKeyDown={e=>{if(e.key==="Enter"||e.key===" "){e.preventDefault();setSelected(v=>!v)}}} className="showcase-select-point"><circle cx="315" cy="171" r="16" fill="#f5d47828" stroke="#f5d478" strokeWidth="1.5"/><circle cx="315" cy="171" r="6.5" fill="#f5d478" stroke="#142b37" strokeWidth="2.5"/></g>}
     <g transform="translate(35 387)" fill="#bdcdd0" fontSize="10"><text y="-7">0</text><text x="67" y="-7">100 m</text><path d="M0 0H80M0 -3V3M40 -3V3M80 -3V3" stroke="#c2cfd2" strokeWidth="1.5" fill="none"/></g>
    </svg>
    <div className="showcase-map-toolbar" aria-hidden="true"><span>⌕</span><span>⊕</span><span>▱</span></div><span className="showcase-coord">N <b>↑</b> · EPSG:4326</span>
    {selected&&layers.point&&<div className="showcase-inspector"><div><span className="showcase-inspector-dot"/><b>Đối tượng đã chọn</b></div><p>Điểm khảo sát minh họa</p><span>Hình học <strong>Point</strong></span></div>}
   </div>
  </div>
  <div className="showcase-status"><span><i/> {Object.values(layers).filter(Boolean).length} lớp hiển thị</span><span>Minh họa tương tác · Không chứa dữ liệu thật</span></div>
 </div>
 <div className="showcase-float"><span><svg viewBox="0 0 24 24"><path d="M12 3 3 7.5 12 12l9-4.5L12 3ZM3 12l9 4.5 9-4.5M3 16.5l9 4.5 9-4.5"/></svg></span><div><b>Làm việc theo layer</b><small>Bật, tắt và chọn trực tiếp</small></div></div>
 </div>;
}

export function AuthComplete(){
 const [status,setStatus]=useState("Đang hoàn tất đăng nhập…");
 useEffect(()=>{const passwordResult=new URLSearchParams(location.search).get("password_result");if(passwordResult){if(window.parent!==window){window.parent.postMessage({type:"ovg:password-result",result:passwordResult},window.location.origin);setStatus("Đã hoàn tất yêu cầu đổi mật khẩu.")}else window.location.replace(appPath+"?password_result="+encodeURIComponent(passwordResult));return}let active=true;getCurrentUser(client).then(()=>{if(!active)return;setStatus("Đăng nhập thành công");if(window.parent!==window)window.parent.postMessage({type:"ovg:auth-complete"},window.location.origin);else if(window.opener)try{window.opener.postMessage({type:"ovg:auth-complete"},window.location.origin)}catch{};if(window.parent===window){window.location.replace(appPath)}}).catch(()=>{if(active)setStatus("Không thể xác nhận phiên. Vui lòng thử lại.")});return()=>{active=false}},[]);
 return <main className="auth-complete"><Brand/><p>{status}</p><a href={appPath} target="_top">Mở không gian làm việc →</a></main>
}

export function Landing({provider,error,invitation,authenticated,onAuth,onWorkspace,onDismissInvitation}:{provider?:string;error:string;invitation:boolean;authenticated:boolean;onAuth:()=>Promise<boolean>;onWorkspace:()=>void;onDismissInvitation:()=>void}){
 const [mode,setMode]=useState<Mode|null>(null),[notice,setNotice]=useState(()=>new URLSearchParams(location.search).has("auth_cancelled")?"Đã hủy đăng nhập. Bạn có thể thử lại.":""),[frameHeight,setFrameHeight]=useState(456);
 const dialog=useRef<HTMLDialogElement>(null),frame=useRef<HTMLIFrameElement>(null),requested=useRef(false),pending=useRef(false),lastTrigger=useRef<HTMLElement|null>(null);
 const open=(next:Mode)=>{if(authenticated){onWorkspace();return}lastTrigger.current=document.activeElement as HTMLElement;setNotice("");requested.current=false;setFrameHeight(next==="register"?620:456);setMode(next)};
 const close=()=>{setMode(null);setNotice("");lastTrigger.current?.focus()};
 useEffect(()=>{if(invitation&&!authenticated){requested.current=false;setMode("login")}},[invitation,authenticated]);
 useEffect(()=>{if(mode){if(!dialog.current?.open)dialog.current?.showModal()}else if(dialog.current?.open)dialog.current.close()},[mode]);
 useEffect(()=>{if(!mode)return;let active=true;const check=async()=>{if(pending.current)return;pending.current=true;try{if(await onAuth()&&active)close()}finally{pending.current=false}};
  const receive=(event:MessageEvent)=>{if(event.origin!==location.origin||event.source!==frame.current?.contentWindow)return;if(event.data?.type==="ovg:auth-complete")void check();if(event.data?.type==="ovg:auth-height"&&typeof event.data.height==="number")setFrameHeight(Math.max(360,Math.min(1100,event.data.height)));if(event.data?.type==="ovg:social-start")setNotice("Đang chuyển đến Google. Sau khi xác thực, bạn sẽ trở lại OpenVietGIS.")};
  window.addEventListener("message",receive);return()=>{active=false;window.removeEventListener("message",receive)}
 },[mode,onAuth]);
 const loaded=()=>{if(!mode||!frame.current)return;try{const doc=frame.current.contentDocument;if(!doc)return;const location=frame.current.contentWindow?.location;if(location?.pathname==="/app"&&location.search.includes("auth_complete")){void onAuth();return}if(mode==="register"&&!requested.current){const register=doc.querySelector<HTMLAnchorElement>("#kc-registration a");if(register){requested.current=true;register.click()}}}catch{/* OAuth consent is opened by Keycloak in a separate window. */}};
 return <div className="landing-page">
 <header className="landing-header"><div className="landing-container landing-nav"><a href="/" className="landing-home" aria-label="OpenVietGIS trang chủ"><Brand/></a><nav aria-label="Điều hướng"><a href="#features">Nền tảng</a><a href="#workflow">Nghiệp vụ</a><a href="/?marketplace=1">Marketplace</a><a href="#about">Bắt đầu</a></nav><div className="landing-actions">{authenticated?<button className="landing-primary" onClick={onWorkspace}>Vào ứng dụng <span>↗</span></button>:<><button className="landing-link-button" onClick={()=>open("login")}>Đăng nhập</button><button className="landing-primary" aria-label="Đăng ký" onClick={()=>open("register")}>Đăng ký <span aria-hidden="true">↗</span></button></>}</div></div></header>
 <main>
 <section className="landing-hero">
 <svg className="atlas-contours" viewBox="0 0 1450 760" preserveAspectRatio="xMidYMid slice" aria-hidden="true"><g fill="none">{Array.from({length:20},(_,i)=><path key={i} d={`M${-180+i*21} 780 C ${-220+i*46} ${400-i*5} ${250+i*19} ${820-i*29} ${280+i*39} ${400-i*13} S ${560+i*36} ${-40+i*8} ${900+i*36} ${80-i*16} S ${1300+i*12} ${300-i*12} 1550 ${-80+i*26}`} stroke={i%5===0?"#70bca6":"#3b786e"} strokeWidth={i%5===0?1.2:.7}/>)}</g></svg>
 <div className="landing-container hero-grid"><div className="hero-copy">
  <div className="landing-kicker"><span><i/> OpenVietGIS Studio</span><span className="kicker-separator"/> Bản đồ &amp; dữ liệu không gian</div>
  <h1>Kiến tạo<br/>không gian.<br/><span>Làm chủ dữ liệu.</span></h1>
  <p>Một không gian làm việc GIS cho đội ngũ Việt Nam. Từ dữ liệu khảo sát đến bản đồ hoàn chỉnh — biên tập, phân tích và cộng tác trên cùng một nền tảng.</p>
  <div className="hero-actions"><button className="landing-primary landing-large" onClick={()=>open("login")}>{authenticated?"Mở workspace":"Bắt đầu ngay"} <span>↗</span></button><a href="#features" className="landing-secondary">Khám phá nền tảng <span>↓</span></a></div>
  <div className="hero-proof"><span><i>✓</i> Làm việc trực tiếp trên bản đồ</span><span><i>✓</i> Phân quyền theo dự án</span></div>
  {invitation&&<div className="landing-invite"><b>Bạn có lời mời tham gia tổ chức.</b><button onClick={()=>open("login")}>Tiếp tục lời mời →</button><button onClick={onDismissInvitation}>Bỏ qua</button></div>}
 </div><MapPreview/></div>
 <div className="landing-container hero-bottom"><span><i className="atlas-live-dot"/> Studio · Demo tương tác</span><span className="hero-bottom-line"/><span>Point &nbsp; / &nbsp; Line &nbsp; / &nbsp; Polygon</span></div>
 </section>
 <section className="landing-features landing-container" id="features"><div className="section-copy"><div><span className="landing-section-kicker">NỀN TẢNG</span><h2>Từ tọa độ đến<br/>quyết định.</h2></div><p>Bốn năng lực cốt lõi, một không gian làm việc liền mạch. Dữ liệu luôn là trung tâm.</p></div><div className="feature-grid">
 {([["layers","Lớp dữ liệu","Nhập, quản lý và hiển thị điểm, đường, vùng.","#e3f5ee","#087b64"],["edit","Biên tập bản đồ","Chọn đối tượng, sửa tọa độ và xem thuộc tính.","#e8f1ff","#2465ad"],["team","Tổ chức & dự án","Phân quyền, mời thành viên và cộng tác theo dự án.","#fff3da","#9b691a"],["plugin","Mở rộng nghiệp vụ","Bổ sung phân hệ chuyên sâu theo nhu cầu.","#e7f3f6","#247f91"]] as const).map(([icon,title,description,bg,color],i)=><article key={title} className="landing-feature"><div className="feature-top"><span className="feature-icon" style={{background:bg,color}}>{icon==="layers"?<svg viewBox="0 0 24 24"><path d="m12 3 9 5-9 5-9-5 9-5Zm-9 9 9 5 9-5M3 16l9 5 9-5"/></svg>:icon==="edit"?<svg viewBox="0 0 24 24"><path d="M4 20h4l12-12-4-4L4 16v4ZM13 7l4 4"/></svg>:icon==="team"?<svg viewBox="0 0 24 24"><circle cx="9" cy="8" r="3"/><path d="M3 20v-3a6 6 0 0 1 12 0v3M17 5a3 3 0 0 1 0 6m1 3a5 5 0 0 1 3 5"/></svg>:<svg viewBox="0 0 24 24"><path d="M12 3v5m0 8v5M3 12h5m8 0h5M8 8h8v8H8z"/></svg>}</span><small>0{i+1}</small></div><h3>{title}</h3><p>{description}</p></article>)}
 </div></section>
 <section className="landing-workflow" id="workflow"><div className="landing-container workflow-grid"><div className="workflow-copy"><span className="landing-section-kicker">LINH HOẠT THEO NGHIỆP VỤ</span><h2>Core GIS mạnh.<br/>Nghiệp vụ linh hoạt.</h2><p>Một nền tảng GIS trung tâm, mở rộng bằng các plugin. Bắt đầu từ dự án của bạn.</p><button className="landing-primary" onClick={()=>open("login")}>Mở không gian làm việc <span>↗</span></button></div>
 <div className="workflow-stack"><article className="workflow-card core"><span className="workflow-tag">GIS CORE</span><h3>Không gian bản đồ</h3><div className="workflow-tags"><span>Layer</span><span>Hình học</span><span>Thuộc tính</span><span>Nhập dữ liệu</span></div></article><div className="workflow-connector"><span/><i>＋</i><span/></div><article className="workflow-card plugin"><span className="workflow-tag">PLUGIN · SỐ NHÀ</span><h3>Quản lý và khảo sát số nhà</h3><div className="workflow-tags"><span>Địa bàn</span><span>Tuyến / hẻm</span><span>Khảo sát</span><span>Đối soát</span></div></article></div>
 </div></section>
 <section className="landing-cta" id="about"><div className="landing-container cta-shell"><div className="cta-content"><span className="landing-section-kicker">OPENVIETGIS STUDIO</span><h2>Bắt đầu xây dựng<br/>không gian dữ liệu của bạn.</h2></div><div className="cta-actions"><button className="landing-primary landing-large" onClick={()=>open("register")}>{authenticated?"Mở workspace":"Tạo tài khoản"} <span>↗</span></button>{!authenticated&&<button className="landing-link-button" onClick={()=>open("login")}>Đã có tài khoản?</button>}</div></div></section>
 </main><footer className="landing-footer"><div className="landing-container"><Brand/><span>© OpenVietGIS · Nền tảng GIS cho mọi dự án</span><a href="#">Về đầu trang ↑</a></div></footer>
 <dialog className="landing-auth" ref={dialog} onCancel={e=>{e.preventDefault();close()}} onClose={()=>setMode(null)} onClick={e=>{if(e.target===dialog.current)close()}} aria-label="Tài khoản OpenVietGIS"><div className="auth-dialog-inner"><div className="auth-dialog-top"><Brand/><button aria-label="Đóng" className="auth-close" onClick={close}>✕</button></div>{mode&&provider?<>{invitation&&<div className="auth-invite-banner"><strong>Bạn được mời tham gia dự án</strong><span>Đăng nhập hoặc đăng ký bằng đúng email nhận lời mời để tiếp tục.</span></div>}<iframe title="Xác thực tài khoản OpenVietGIS" ref={frame} key={mode} src={loginUrl(provider)} onLoad={loaded} style={{height:frameHeight}}/><p className="auth-dialog-notice" role="status">{notice||"Xác thực bảo mật · OpenVietGIS"}</p></>:<p className="auth-dialog-notice" role="status">{error||"Đang kết nối dịch vụ xác thực…"}</p>}</div></dialog>
 {error&&!mode&&<div className="landing-error" role="alert">{error}</div>}
 <LandingSwarm enabled={!mode&&!invitation}/>
 </div>
}
