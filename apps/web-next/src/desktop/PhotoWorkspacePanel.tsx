import {useEffect,useRef,useState} from "react";
import {desktop,type PhotoInventory,type PhotoDataset,type PhotoProgress,type PhotoRenderProgress,type PhotoRenderReceipt} from "./transport";

export default function PhotoWorkspacePanel({handle,selected,onSelect}:{handle:string;selected?:string;onSelect:(id:string|undefined)=>void}){
 const [inventory,setInventory]=useState<PhotoInventory|null>(null);
 const [busy,setBusy]=useState(false),[error,setError]=useState("");
 const alive=useRef(true),locked=useRef(false);
 const [dataset,setDataset]=useState<PhotoDataset|null>(null),[progress,setProgress]=useState<PhotoProgress|null>(null);
 const request=useRef<string|null>(null);
 const [renders,setRenders]=useState<PhotoRenderReceipt[]>([]),[renderProgress,setRenderProgress]=useState<PhotoRenderProgress|null>(null);
 const [rendering,setRendering]=useState(false);
 useEffect(()=>{alive.current=true;void desktop.photoPreviews(handle).then(v=>{if(alive.current)setRenders(v)}).catch(e=>{if(alive.current)setError(String(e))});void desktop.photoDataset(handle).then(d=>{if(alive.current)setDataset(d)}).catch(e=>{if(alive.current)setError(String(e))});return()=>{alive.current=false;if(request.current)void desktop.photoCancel(handle,request.current).catch(()=>{})}},[handle]); 
 const stage=async()=>{
  if(locked.current||dataset)return;
  locked.current=true;setBusy(true);setError("");setProgress(null);
  const id=crypto.randomUUID();request.current=id;
  const timer=setInterval(()=>{void desktop.photoStatus(handle,id).then(p=>{if(alive.current&&request.current===id&&p)setProgress(p)}).catch(()=>{})},250);
  try{const d=await desktop.photoStage(handle,id);if(alive.current&&d)setDataset(d);}
  catch(e){if(alive.current)setError(String(e));}
  finally{clearInterval(timer);request.current=null;locked.current=false;if(alive.current){setBusy(false);setProgress(null);}}
 };
 const render=async()=>{
  if(locked.current)return;
  locked.current=true;setBusy(true);setRendering(true);setError("");setRenderProgress(null);
  const id=crypto.randomUUID();request.current=id;
  const timer=setInterval(()=>{void desktop.photoRenderStatus(handle,id).then(p=>{if(alive.current&&request.current===id&&p)setRenderProgress(p)}).catch(()=>{})},400);
  try{const r=await desktop.photoRenderPrepared(handle,id);if(alive.current&&r)setRenders(v=>[...v,r]);}
  catch(e){if(alive.current)setError(String(e));}
  finally{clearInterval(timer);request.current=null;locked.current=false;if(alive.current){setBusy(false);setRendering(false);setRenderProgress(null);}}
 };
 const inspect=async()=>{
  if(locked.current)return;locked.current=true;setBusy(true);setError("");
  try{const result=await desktop.photoInspect(handle);if(alive.current&&result)setInventory(result);}
  catch(e){if(alive.current)setError(String(e));}
  finally{locked.current=false;if(alive.current)setBusy(false);}
 };
 return <details>
  <summary>Ghép ảnh flycam · Photogrammetry</summary>
  <p>Kiểm tra ảnh JPEG cục bộ trước khi xử lý. Không tải ảnh lên mạng, không sửa ảnh gốc.</p>
  <button disabled={busy} onClick={()=>void inspect()}>{busy?"Đang kiểm tra…":"Chọn ảnh flycam"}</button>
  <button disabled={busy||!!dataset} onClick={()=>void stage()}>Lưu bộ ảnh vào dự án (tối đa 8 GiB)</button>
  {busy&&request.current&&<button onClick={()=>{if(request.current)void desktop.photoCancel(handle,request.current).catch(e=>setError(String(e)) )}}>{rendering?"Hủy render":"Hủy nhập ảnh"}</button>}
  {progress&&<div role="status"><progress max={Math.max(1,progress.total_bytes)} value={progress.copied}/><span>{progress.completed}/{progress.total_files} ảnh · {(progress.copied/1024**2).toFixed(1)} MiB</span></div>}
  {dataset&&<p>Đã lưu {dataset.count} ảnh · {(dataset.bytes/1024**2).toFixed(1)} MiB · SHA-256 {dataset.manifest_sha256.slice(0,12)}…</p>}
  {inventory&&<dl>
   <dt>Số ảnh</dt><dd>{inventory.count.toLocaleString("vi-VN")}</dd>
   <dt>Dung lượng</dt><dd>{(inventory.bytes/1024**2).toFixed(1)} MiB</dd>
   <dt>Tổng pixel</dt><dd>{(inventory.pixels/1e6).toFixed(1)} MP</dd>
   <dt>Kích thước</dt><dd>{inventory.min_width}–{inventory.max_width} × {inventory.min_height}–{inventory.max_height} px</dd>
   <dt>Ảnh có GPS</dt><dd>{inventory.gps_count}/{inventory.count}</dd>
   <dt>EXIF cần kiểm tra</dt><dd>{inventory.metadata_errors}</dd>
   <dt>Hướng ảnh khác mặc định</dt><dd>{inventory.non_default_orientation}</dd>
  </dl>}
  <section>
   <h3>Render CPU thử nghiệm</h3>
   <p>Chọn thư mục bộ dữ liệu đã chuẩn bị (manifest.json, camera pinhole, DEM và ảnh đã khử méo). Không nhận trực tiếp ảnh flycam thô.</p>
   <button disabled={busy||renders.length>=4} onClick={()=>void render()}>Render bộ ảnh đã chuẩn bị</button>
   {rendering&&<p role="status">{renderProgress?.total_tiles?<><progress max={renderProgress.total_tiles} value={renderProgress.completed_tiles}/> {renderProgress.completed_tiles}/{renderProgress.total_tiles} tile đã ghi</>:"Đang chọn hoặc kiểm tra bộ dữ liệu…"}</p>}
   {renders.map(r=><p key={r.job_id}><button aria-pressed={selected===r.job_id} onClick={()=>onSelect(selected===r.job_id?undefined:r.job_id)}>{selected===r.job_id?"Ẩn trên bản đồ":"Xem trên bản đồ"}</button> Preview {r.job_id.slice(0,8)} · EPSG:{r.result.epsg} · {r.result.tiles} tile · Độ phủ {(100*r.result.covered/r.result.pixels).toFixed(1)}% · {(r.result.written_bytes/1024**2).toFixed(1)} MiB</p>)}
   <p>Bản xem trước kỹ thuật cục bộ, raster XYZ lấy mẫu theo mức zoom. Chưa nghiệm thu độ chính xác tọa độ, chưa dùng đo đạc. GPU chưa bật.</p>
  </section>
  {error&&<p role="alert">{error}</p>}
 </details>;
}
