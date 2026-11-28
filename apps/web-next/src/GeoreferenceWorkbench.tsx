import {useEffect,useRef,useState} from "react";
import type {RefObject} from "react";
import {ApiClient,loadCsrfToken} from "@openvietgis/client-sdk";
import * as ml from "./map-surface";
import {sha256,validatePreview,validateGcps,validateGeoref,georefBlob,mapTarget,type Gcp,type GeorefPreview,type GeorefResult} from "./georeference-contract";
type PointDraft={id:string;pixel:string;line:string;x:string;y:string};
const blank=():PointDraft=>({id:crypto.randomUUID(),pixel:"",line:"",x:"",y:""});
const parse=(p:PointDraft):Gcp=>[p.pixel,p.line,p.x,p.y].map(s=>s.trim()===""?NaN:Number(s)) as Gcp;
const layerId="ovg-georeference-footprint";
const fmt=(n:number)=>n.toLocaleString("vi-VN",{maximumSignificantDigits:8});
export function GeoreferenceWorkbench({client,org,pid,map,ready,close,onBusy}:{client:ApiClient;org:string;pid:string;map:RefObject<ml.Map|null>;ready:boolean;close:()=>void;onBusy:(b:boolean)=>void}){
 const [file,setFile]=useState<File|null>(null),[preview,setPreview]=useState<GeorefPreview|null>(null),[points,setPoints]=useState<PointDraft[]>(()=>[blank(),blank(),blank()]),[active,setActive]=useState(0),[epsg,setEpsg]=useState(4326),[result,setResult]=useState<GeorefResult|null>(null),[blob,setBlob]=useState<Blob|null>(null),[accepted,setAccepted]=useState(false),[busy,setBusy]=useState(false),[error,setError]=useState("");
 const running=useRef(false),abort=useRef<AbortController|null>(null),generation=useRef(0);
 useEffect(()=>()=>{++generation.current;abort.current?.abort();onBusy(false)},[org,pid]);
 useEffect(()=>{onBusy(busy)},[busy,onBusy]);
 const invalidate=()=>{++generation.current;setResult(null);setBlob(null);setAccepted(false);setError("")};
 const update=(index:number,delta:Partial<PointDraft>)=>{invalidate();setPoints(old=>old.map((p,i)=>i===index?{...p,...delta}:p))};
 const request=async(mode:"inspect"|"solve")=>{
  if(running.current||!file)return;
  if(!/\.tiff?$/i.test(file.name)||file.size<=0||file.size>32*1024*1024){setError("Chọn TIFF tối đa 32 MiB.");return}
  const seq=++generation.current,ctrl=new AbortController();abort.current=ctrl;running.current=true;setBusy(true);setError("");setResult(null);setBlob(null);setAccepted(false);
  try{
   const digest=await sha256(await file.arrayBuffer()),form=new FormData();form.set("file",file);form.set("mode",mode);
   let gcps:Gcp[]=[];
   if(mode==="solve"){if(!preview||preview.source_sha256!==digest)throw Error("Cần xem trước lại tệp.");gcps=validateGcps(points.map(parse),preview,epsg);form.set("source_sha256",digest);form.set("epsg",String(epsg));form.set("points",JSON.stringify(gcps))}
   await loadCsrfToken(client);
   if(ctrl.signal.aborted||seq!==generation.current)return;
   const raw=await client.request<unknown>("/organizations/"+encodeURIComponent(org)+"/projects/"+encodeURIComponent(pid)+"/raster/georeference",{method:"POST",body:form,signal:ctrl.signal});
   if(ctrl.signal.aborted||seq!==generation.current)return;
   if(mode==="inspect"){const p=validatePreview(raw,digest);if(p.mode!=="inspect")throw Error("Phản hồi không phải xem trước.");setPreview(p)}
   else {const r=validateGeoref(raw,preview!,gcps,epsg),verified=await georefBlob(r);if(ctrl.signal.aborted||seq!==generation.current)return;setResult(r);setBlob(verified)}
  }catch(e){if(!ctrl.signal.aborted&&seq===generation.current)setError(e instanceof Error?e.message:String(e))}
  finally{running.current=false;if(!ctrl.signal.aborted&&seq===generation.current)setBusy(false)}
 };
 const pickTarget=()=>{try{const c=map.current?.getCenter().wrap();if(!c)throw Error("Bản đồ chưa sẵn sàng.");const [x,y]=mapTarget(c.lng,c.lat,epsg);update(active,{x:String(x),y:String(y)})}catch(e){setError(String(e))}};
 useEffect(()=>{const m=map.current;if(!m||!ready||!result)return;
  const remove=()=>{try{if(m.getLayer(layerId))m.removeLayer(layerId);if(m.getSource(layerId))m.removeSource(layerId)}catch{}};
  const draw=()=>{remove();try{const corners=result.corners; m.addSource(layerId,{type:"geojson",data:{type:"Feature",properties:{},geometry:{type:"Polygon",coordinates:[[...corners,corners[0]!] ]}}});m.addLayer({id:layerId,type:"line",source:layerId,paint:{"line-color":"#e65427","line-width":3,"line-dasharray":[2,2]}})}catch{}};
  draw();m.on("style.load",draw);return()=>{m.off("style.load",draw);remove()}
 },[map,ready,result]);
 const zoom=()=>{if(!result||!map.current)return;const bounds=new ml.LngLatBounds();result.corners.forEach(p=>bounds.extend(p));map.current.fitBounds(bounds,{padding:65,duration:300,maxZoom:19})};
 const download=()=>{if(!blob||!result||!accepted)return;const url=URL.createObjectURL(blob),a=document.createElement("a");a.href=url;a.download="georeferenced-epsg"+result.epsg+".tif";a.click();setTimeout(()=>URL.revokeObjectURL(url),1000)};
 return <aside className="panel pro-dock raster-workbench" role="dialog" aria-label="Georeferencer"><header><div><small>GIS CORE · GCP / AFFINE</small><b>Định vị ảnh Raster</b></div><button aria-label="Đóng Georeferencer" disabled={busy} onClick={close}>✕</button></header><div className="pro-dock-body">
 <p className="pro-note">TIFF chưa có định vị, tối đa 2 triệu pixel / 16 band. GCP dùng tọa độ góc pixel (0,0 ở góc trên trái), phép affine bậc 1. Không ghi đè ảnh đã có CRS, không nội suy lại ảnh.</p>
 <fieldset disabled={busy}><label>Ảnh TIFF<input type="file" accept=".tif,.tiff,image/tiff" onChange={e=>{invalidate();setFile(e.target.files?.[0]??null);setPreview(null);setPoints([blank(),blank(),blank()]);setActive(0)}}/></label><button disabled={!file} onClick={()=>void request("inspect")}>Đọc ảnh và kích thước</button>
 {preview&&<><p>{preview.width} × {preview.height} · {preview.bands} band · Band 1 xem trước</p><div className="georef-image" style={{position:"relative",width:"100%"}}><img src={"data:image/png;base64,"+preview.png} alt="Ảnh nguồn để đặt điểm GCP" style={{display:"block",width:"100%",cursor:"crosshair"}} onClick={e=>{const r=e.currentTarget.getBoundingClientRect();update(active,{pixel:((e.clientX-r.left)/r.width*preview.width).toFixed(3),line:((e.clientY-r.top)/r.height*preview.height).toFixed(3)})}}/>{points.map((p,i)=>{const [x,y]=parse(p);return Number.isFinite(x)&&Number.isFinite(y)&&x>=0&&x<=preview.width&&y>=0&&y<=preview.height?<span key={p.id} style={{position:"absolute",left:(100*x/preview.width)+"%",top:(100*y/preview.height)+"%",transform:"translate(-50%,-50%)",pointerEvents:"none",background:i===active?"#e65427":"#1765ad",color:"white",borderRadius:"50%",padding:"2px 5px"}}>{i+1}</span>:null})}</div>
 <label>CRS đích<select value={epsg} onChange={e=>{invalidate();setEpsg(Number(e.target.value));setPoints(old=>old.map(p=>({...p,x:"",y:""})))}}><option value={4326}>EPSG:4326 · kinh/vĩ độ (°)</option><option value={3857}>EPSG:3857 · Web Mercator (m chiếu)</option></select></label>
 <p>Chọn điểm đang sửa, bấm trên ảnh hoặc nhập pixel/line. Pan bản đồ tới điểm tương ứng rồi lấy tâm bản đồ, hoặc nhập tọa độ đích chính xác.</p>
 {points.map((p,i)=><fieldset key={p.id} className="georef-point"><legend><button type="button" aria-pressed={active===i} onClick={()=>setActive(i)}>GCP {i+1}</button>{points.length>3&&<button type="button" onClick={()=>{invalidate();setPoints(old=>old.filter(x=>x.id!==p.id));setActive(0)}}>Xóa</button>}</legend>{(["pixel","line","x","y"] as const).map((key,j)=><label key={key}>{["Pixel X","Line Y","Tọa độ X","Tọa độ Y"][j]}<input aria-label={"GCP "+(i+1)+" "+key} inputMode="decimal" value={p[key]} onChange={e=>update(i,{[key]:e.target.value})}/></label>)}{result&&<small>Sai số {fmt(result.residuals[i]??0)} {epsg===4326?"°":"m chiếu"} · {fmt(result.pixel_residuals[i]??0)} px</small>}</fieldset>)}
 <div className="live-actions"><button type="button" disabled={points.length>=32} onClick={()=>{invalidate();setActive(points.length);setPoints(old=>[...old,blank()])}}>＋ GCP</button><button type="button" onClick={pickTarget}>Lấy tọa độ tâm bản đồ cho GCP {active+1}</button></div>
 <button type="button" onClick={()=>void request("solve")}>Tính affine và tạo GeoTIFF</button></>}</fieldset>
 {busy&&<p role="status">Đang xử lý ảnh…</p>}{error&&<p role="alert">{error}</p>}
 {result&&<section><b>RMS: {fmt(result.rms)} {result.epsg===4326?"°":"m chiếu"} · {fmt(result.pixel_rms)} px</b><p>{result.redundant?"Có điểm dư để đánh giá sai số nội tại.":"Chỉ có 3 GCP: sai số gần 0 không chứng minh độ chính xác."} Cần đối chiếu điểm kiểm tra độc lập. Đường bao cam là footprint, không phải ảnh được warp.</p><button onClick={zoom}>Zoom tới ranh ảnh</button><details><summary>Affine GDAL (6 hệ số)</summary><code>{result.transform.map(x=>x.toPrecision(12)).join(", ")}</code></details><label><input type="checkbox" checked={accepted} onChange={e=>setAccepted(e.target.checked)}/> Tôi đã kiểm tra GCP, CRS và sai số.</label><button disabled={!accepted||!blob} onClick={download}>Tải GeoTIFF đã kiểm tra SHA-256</button><small>Không tự lưu vào dự án. Nếu ảnh xoay/nghiêng cần warp trước khi dùng công cụ raster chỉ hỗ trợ north-up.</small></section>}
 </div></aside>
}
