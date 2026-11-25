import {uiConfirm,uiPrompt} from "./ui-dialog";
import React,{useEffect,useMemo,useState} from "react";
import {ApiClient,ApiError,listSpatialSources,listSpatialLayers,createSpatialLayer,renameSpatialLayer,retireSpatialLayer,loadCsrfToken,type SpatialSource,type SpatialLayer} from "@openvietgis/client-sdk";

// Spec 058: QGIS-style source & layer manager replacing the legacy developer catalog page.
const client=new ApiClient({baseUrl:"/api/v1"});
const FAMILY:Record<string,string>={Point:"Điểm",LineString:"Đường",Polygon:"Vùng",MultiPoint:"Điểm",MultiLineString:"Đường",MultiPolygon:"Vùng"};
const STATE:Record<string,string>={ready:"Sẵn sàng",pending:"Đang kiểm tra",failed:"Lỗi",invalid:"Không hợp lệ"};
const n=(s:string)=>s.normalize("NFD").replace(/\p{M}/gu,"").replace(/đ/gi,"d").toLowerCase();
async function all<T>(page:(after?:string)=>Promise<{items:T[];next_after_id?:string|null}>){const out:T[]=[];let after:string|undefined;for(let i=0;i<20;i++){const r=await page(after);out.push(...r.items);if(!r.next_after_id)break;after=r.next_after_id}return out}
const reloadMap=()=>window.dispatchEvent(new CustomEvent("ovg:menu-command",{detail:"RELOAD"}));

export function CatalogManager({org,pid}:{org:string;pid:string}){
 const [sources,setSources]=useState<SpatialSource[]>([]),[layers,setLayers]=useState<SpatialLayer[]>([]),[q,setQ]=useState(""),[busy,setBusy]=useState(false),[msg,setMsg]=useState(""),[err,setErr]=useState(""),[rev,setRev]=useState(0),[loading,setLoading]=useState(true);
 useEffect(()=>{let live=true;setLoading(true);Promise.all([all(a=>listSpatialSources(client,org,pid,{limit:200,...(a?{after_id:a}:{})})),all(a=>listSpatialLayers(client,org,pid,{limit:200,...(a?{after_id:a}:{})}))]).then(([s,l])=>{if(live){setSources(s.filter(x=>x.lifecycle==="active"));setLayers(l.filter(x=>x.lifecycle==="active"))}}).catch(e=>{if(live)setErr(e instanceof ApiError?"Không tải được danh mục ("+e.status+").":"Không tải được danh mục.")}).finally(()=>{if(live)setLoading(false)});return()=>{live=false}},[org,pid,rev]);
 const usage=useMemo(()=>{const m=new Map<string,number>();for(const l of layers)m.set(l.source_id,(m.get(l.source_id)??0)+1);return m},[layers]);
 const sourceName=(id:string)=>sources.find(s=>s.id===id)?.name??"—";
 const match=(text:string)=>!q||n(text).includes(n(q));
 const run=async(fn:()=>Promise<unknown>,ok:string)=>{if(busy)return;setBusy(true);setErr("");setMsg("");try{await loadCsrfToken(client);await fn();setMsg(ok);setRev(x=>x+1);reloadMap()}catch(e){setErr(e instanceof ApiError?(e.status===409?"Dữ liệu vừa thay đổi, đã tải lại danh sách.":"Không thực hiện được ("+e.status+")."):"Không thực hiện được.");setRev(x=>x+1)}finally{setBusy(false)}};
 const addLayer=async (s:SpatialSource)=>{const name=(await uiPrompt("Tên lớp mới từ nguồn \""+s.name+"\"",s.name))?.trim();if(name)void run(()=>createSpatialLayer(client,org,pid,{source_id:s.id,name}),"Đã thêm lớp \""+name+"\" vào bản đồ.")};
 const rename=async (l:SpatialLayer)=>{const name=(await uiPrompt("Tên mới cho lớp",l.name))?.trim();if(name&&name!==l.name)void run(()=>renameSpatialLayer(client,org,pid,l.id,{expected_revision:l.layer_revision,name}),"Đã đổi tên lớp.")};
 const remove=async (l:SpatialLayer)=>{if((await uiConfirm("Gỡ lớp \""+l.name+"\" khỏi dự án? Nguồn dữ liệu và đối tượng vẫn được giữ, có thể tạo lại lớp từ nguồn.")))void run(()=>retireSpatialLayer(client,org,pid,l.id,{expected_revision:l.layer_revision}),"Đã gỡ lớp.")};
 const shownSources=sources.filter(s=>match(s.name)),shownLayers=layers.filter(l=>match(l.name+" "+sourceName(l.source_id)));
 return <div className="cat-mgr">
  <div className="cat-bar"><input type="search" aria-label="Lọc nguồn và lớp" placeholder="Lọc theo tên…" value={q} onChange={e=>setQ(e.target.value)}/><button type="button" onClick={()=>setRev(x=>x+1)} disabled={loading}>{loading?"Đang tải…":"↻ Tải lại"}</button></div>
  {err&&<p className="error" role="alert">{err}</p>}{msg&&<p className="cat-ok" role="status">{msg}</p>}
  <h3>Nguồn dữ liệu <small>{sources.length}</small></h3>
  <div className="cat-scroll"><table className="cat-table"><thead><tr><th>Tên nguồn</th><th>Hình học</th><th>CRS</th><th className="num">Đối tượng</th><th>Trạng thái</th><th className="num">Lớp đang dùng</th><th/></tr></thead>
  <tbody>{shownSources.map(s=>{const fam=s.schema?.geometry_family??"",ready=s.validation_state==="ready";return <tr key={s.id}><td title={s.name}>{s.name}</td><td>{FAMILY[fam]??(fam||"—")}</td><td title={s.stored_crs?.name??""}>{s.stored_crs?.code?"EPSG:"+s.stored_crs.code:"—"}</td><td className="num">{s.feature_count?.toLocaleString("vi-VN")??"—"}</td><td><span className={"cat-state "+(ready?"ok":"warn")} title={typeof s.validation_reason==="string"?s.validation_reason:s.validation_reason?.message??""}>{STATE[s.validation_state]??s.validation_state}</span></td><td className="num">{usage.get(s.id)??0}</td><td className="act"><button type="button" disabled={busy||!ready} title={ready?"Tạo lớp mới hiển thị nguồn này":"Nguồn chưa sẵn sàng"} onClick={()=>addLayer(s)}>＋ Tạo lớp</button></td></tr>})}
  {!shownSources.length&&<tr><td colSpan={7} className="empty">{loading?"Đang tải…":q?"Không có nguồn khớp.":"Chưa có nguồn. Dùng Lớp ▸ Thêm dữ liệu để nhập tệp."}</td></tr>}</tbody></table></div>
  <h3>Lớp trong dự án <small>{layers.length}</small></h3>
  <div className="cat-scroll"><table className="cat-table"><thead><tr><th>Tên lớp</th><th>Nguồn</th><th className="num">Đối tượng</th><th/></tr></thead>
  <tbody>{shownLayers.map(l=><tr key={l.id}><td title={l.name}>{l.name}</td><td title={sourceName(l.source_id)}>{sourceName(l.source_id)}</td><td className="num">{l.source?.feature_count?.toLocaleString("vi-VN")??"—"}</td><td className="act"><button type="button" disabled={busy} onClick={()=>rename(l)}>Đổi tên</button><button type="button" className="danger" disabled={busy} onClick={()=>remove(l)}>Gỡ</button></td></tr>)}
  {!shownLayers.length&&<tr><td colSpan={4} className="empty">{loading?"Đang tải…":q?"Không có lớp khớp.":"Chưa có lớp nào."}</td></tr>}</tbody></table></div>
  <p className="cat-note">Một nguồn có thể hiển thị bằng nhiều lớp (mỗi lớp một kiểu ký hiệu) mà không sao chép dữ liệu. Gỡ lớp không xóa nguồn.</p>
 </div>;
}
