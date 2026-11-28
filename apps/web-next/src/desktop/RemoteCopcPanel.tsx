import {useEffect,useRef,useState} from "react";
import {remoteCopc,type RemoteCopcInfo} from "./transport";
export default function RemoteCopcPanel({handle}:{handle:string}){
 const [enabled,setEnabled]=useState(false),[url,setUrl]=useState(""),[meta,setMeta]=useState<RemoteCopcInfo>(),[bbox,setBbox]=useState(""),[level,setLevel]=useState("0"),[busy,setBusy]=useState(false),[message,setMessage]=useState(""),[points,setPoints]=useState<number[][]>([]);
 const request=useRef<string|null>(null),alive=useRef(true);
 useEffect(()=>{alive.current=true;void remoteCopc.status().then(v=>{if(alive.current)setEnabled(v)}).catch(()=>{});return()=>{alive.current=false;if(request.current)void remoteCopc.cancel(request.current).catch(()=>{})}},[]);
 const run=async(f:(id:string)=>Promise<void>)=>{if(request.current)return;const id=crypto.randomUUID();request.current=id;setBusy(true);setMessage("");try{await f(id)}catch(e){if(alive.current)setMessage(String(e))}finally{request.current=null;if(alive.current)setBusy(false)}};
 const inspect=()=>run(async(id)=>{if(!window.confirm("Kết nối nguồn COPC công khai qua HTTPS?"))return;setMeta(undefined);setPoints([]);const m=await remoteCopc.inspect(handle,url.trim(),id);if(!alive.current)return;setMeta(m);const {center,halfsize}=m.info;setBbox([...center.map(v=>v-halfsize),...center.map(v=>v+halfsize)].join(","))});
 const query=()=>run(async(id)=>{if(!meta)return;const b=bbox.split(",").map(Number),n=Number(level);if(b.length!==6||b.some(v=>!Number.isFinite(v))||!Number.isInteger(n)||n<0||n>16)throw Error("INVALID_BOUNDS_OR_LOD");const buf=await remoteCopc.query(handle,url.trim(),meta.identity,b as [number,number,number,number,number,number],n,id);if(!alive.current)return;const d=new DataView(buf),out:number[][]=[];for(let i=0;i<buf.byteLength;i+=24)out.push([d.getFloat64(i,true),d.getFloat64(i+8,true),d.getFloat64(i+16,true)]);setPoints(out);setMessage(out.length+" điểm XYZ; tọa độ gốc, chưa xác nhận CRS.")});
 return <details><summary>Remote COPC (thử nghiệm)</summary><p>LAS 1.4 format 6, BBox XYZ và LOD; tối đa 2048 điểm mỗi yêu cầu. Chưa bật mặc định, không tự tải URL.</p>
 <input aria-label="URL COPC" value={url} disabled={busy||!enabled} onChange={e=>{setUrl(e.target.value);setMeta(undefined);setPoints([])}} maxLength={2048}/>
 <button disabled={!enabled||busy||!url.trim()} onClick={()=>void inspect()}>Kiểm tra COPC</button>{meta&&<><pre>{JSON.stringify(meta.info,null,2)}</pre><label>BBox XYZ <input value={bbox} disabled={busy} onChange={e=>setBbox(e.target.value)}/></label><label>LOD <input value={level} disabled={busy} onChange={e=>setLevel(e.target.value)}/></label><button disabled={busy} onClick={()=>void query()}>Đọc điểm</button></>}
 {busy&&<button onClick={()=>{if(request.current)void remoteCopc.cancel(request.current).catch(()=>{})}}>Hủy</button>}
 <p role="status">{message}</p>{points.length>0&&<pre>{JSON.stringify(points.slice(0,16),null,2)}</pre>}</details>
}
