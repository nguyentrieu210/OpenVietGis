import {useEffect,useRef,useState} from "react";
import {nativeCad} from "./transport";
export default function NativeCadBridgePanel({handle}:{handle:string}){
 const [status,setStatus]=useState<Awaited<ReturnType<typeof nativeCad.status>>>(),[busy,setBusy]=useState(false),[message,setMessage]=useState("");
 const request=useRef<string|null>(null),alive=useRef(true);
 useEffect(()=>{alive.current=true;void nativeCad.status().then(s=>{if(alive.current)setStatus(s)}).catch(e=>{if(alive.current)setMessage(String(e))});return()=>{alive.current=false;if(request.current)void nativeCad.cancel(request.current).catch(()=>{})}},[]);
 const convert=async()=>{if(request.current||!status?.available||!window.confirm("Chuyển đổi bản sao DWG/DXF bằng OpenVietCAD native? File gốc không bị thay đổi."))return;
  const id=crypto.randomUUID();request.current=id;setBusy(true);setMessage("");
  try{const r=await nativeCad.convert(handle,id);if(r&&alive.current)setMessage("Đã xuất "+r.format.toUpperCase()+" · "+r.bytes+" byte · SHA-256 "+r.sha256)}
  catch(e){if(alive.current)setMessage(String(e))}finally{request.current=null;if(alive.current)setBusy(false)}
 };
 return <details><summary>OpenVietCAD native bridge</summary>
 <p>{status?.available?"Có sidecar CAD":"Chưa đóng gói sidecar CAD"} · {status?.source_revision.slice(0,12)}</p>
 <p>Chuyển đổi DWG ↔ DXF qua engine CAD riêng. Không tự chạy script, không ghi đè. Chưa phải editor CAD nhúng.</p>
 <button disabled={!status?.available||busy} onClick={()=>void convert()}>Chuyển đổi DWG / DXF</button>{busy&&<button onClick={()=>{if(request.current)void nativeCad.cancel(request.current).catch(()=>{})}}>Hủy</button>}
 <p role="status">{message}</p></details>
}
