import {useEffect,useRef,useState} from "react";
import {attachDeveloperV2,decideDeveloperV2,exportDeveloperV2,getDeveloperV2,listDeveloperV2,loadCsrfToken,withdrawDeveloperV2,type ApiClient,type DeveloperState,type DeveloperV2Submission} from "@openvietgis/client-sdk";
import signerUrl from "../../../packages/plugin-sdk/contribution_sign.py?url&no-inline";

function download(name:string,value:string){
 const url=URL.createObjectURL(new Blob([value],{type:"application/octet-stream"}));const a=document.createElement("a");a.href=url;a.download=name;a.click();setTimeout(()=>URL.revokeObjectURL(url),1000);
}
export function DeveloperV2Panel({client,state,review}:{client:ApiClient;state:DeveloperState;review:boolean}){
 const [rows,setRows]=useState<DeveloperV2Submission[]>([]),[offset,setOffset]=useState(0),[next,setNext]=useState<number|null>(null),[reload,setReload]=useState(0);
 const [parent,setParent]=useState(""),[bundle,setBundle]=useState(""),[detail,setDetail]=useState<DeveloperV2Submission|null>(null),[note,setNote]=useState(""),[message,setMessage]=useState(""),[busy,setBusy]=useState(false);
 const inFlight=useRef(false),version=useRef(0);
 useEffect(()=>{let active=true;setRows([]);setDetail(null);setNote("");listDeveloperV2(client,review,"all",offset).then(r=>{if(active){setRows(r.items);setNext(r.next_offset)}}).catch(()=>{if(active)setMessage("Không tải được hồ sơ v2.")});return()=>{active=false}},[client,review,offset,reload]);
 useEffect(()=>()=>{++version.current},[]);
 const run=async(fn:(current:()=>boolean)=>Promise<void>)=>{if(inFlight.current)return;inFlight.current=true;const id=++version.current;const current=()=>id===version.current;setBusy(true);setMessage("");try{await loadCsrfToken(client);if(current())await fn(current)}catch{if(current())setMessage("Yêu cầu không thành công. Kiểm tra chữ ký, quyền, trạng thái hồ sơ hoặc kết nối.")}finally{if(current()){inFlight.current=false;setBusy(false)}}};
 const changed=()=>{setDetail(null);setNote("");setReload(v=>v+1)};
 const send=()=>void run(async current=>{if(!parent||new TextEncoder().encode(bundle).length>512*1024)throw Error();const b=JSON.parse(bundle);if(!b||typeof b!=="object"||Array.isArray(b))throw Error();await attachDeveloperV2(client,parent,b);if(current()){setBundle("");changed();setMessage("Đã gửi gói v2 để xét duyệt độc lập.")}});
 const inspect=(r:DeveloperV2Submission)=>void run(async current=>{const d=await getDeveloperV2(client,r.submission_id);if(current()){setDetail(d);setNote("")}});
 const decide=(decision:"approved"|"rejected")=>{if(!detail)return;void run(async current=>{await decideDeveloperV2(client,detail.submission_id,decision,note.trim());if(current()){changed();setMessage("Đã lưu quyết định kiểm duyệt.")}})};
 const withdraw=()=>{if(!detail)return;void run(async current=>{await withdrawDeveloperV2(client,detail.submission_id,note.trim());if(current()){changed();setMessage("Đã ghi nhận yêu cầu. Bản đã phát hành vẫn cần catalog mới do operator ký để ngừng phân phối.")}})};
 const exportBundle=()=>{if(!detail)return;void run(async current=>{const r=await exportDeveloperV2(client,detail.submission_id);if(current()){download("publisher-bundle.json",JSON.stringify(r.bundle,null,2));download("publisher-public.txt",r.publisher_public_key+"\n");setMessage("Đã xuất gói đã duyệt. Ký review và catalog ngoại tuyến trước khi phát hành.")}})};
 return <section className="publisher-panel"><h2>{review?"Kiểm duyệt declarative v2":"Gói plugin declarative v2"}</h2>
 <p>Gói không chứa mã thực thi tải về. Hồ sơ được gắn với đúng manifest v1, ký bằng Ed25519. Không tải khóa riêng lên website.</p>
 <a className="publisher-link" href={signerUrl} download="contribution_sign.py">Tải công cụ ký v2</a>
 {!review&&state.publisher&&<><label>Hồ sơ manifest v1<select value={parent} disabled={busy} onChange={e=>setParent(e.target.value)}><option value="">Chọn phiên bản</option>{state.submissions.filter(r=>r.status==="pending"||r.status==="approved").map(r=><option key={r.id} value={r.id}>{r.manifest.extension_key} · {r.manifest.version}</option>)}</select></label>
 <label>Gói JSON đã ký<textarea rows={5} maxLength={524288} value={bundle} disabled={busy} onChange={e=>setBundle(e.target.value)}/></label>
 <label>Chọn tệp bundle<input type="file" accept=".json,application/json" disabled={busy} onChange={async e=>{const file=e.target.files?.[0];if(file&&file.size<=524288)setBundle(await file.text());else setMessage("Giới hạn gói 512 KiB.");e.target.value=""}}/></label>
 <button disabled={busy||!parent||!bundle.trim()} onClick={send}>Gửi gói v2</button></>}
 <h3>{review?"Hàng đợi v2":"Hồ sơ v2 của tôi"}</h3>{rows.map(r=><article className="publisher-release" key={r.submission_id}><b>{r.extension_key} · {r.version}</b><p>v1: {r.parent_status} · v2: {r.status}{r.withdrawal_requested?" · Yêu cầu rút phát hành":""}</p><code className="publisher-key">{r.release_digest}</code><button disabled={busy} onClick={()=>inspect(r)}>Xem hồ sơ</button></article>)}
 <div className="publisher-controls"><button disabled={busy||offset===0} onClick={()=>setOffset(v=>Math.max(0,v-100))}>Trang trước</button><button disabled={busy||next===null} onClick={()=>setOffset(next??offset)}>Trang sau</button><button disabled={busy} onClick={()=>setReload(v=>v+1)}>Làm mới</button></div>
 {detail&&<article className="publisher-release"><h3>{detail.extension_key} · {detail.version}</h3><p>Trạng thái: {detail.status}. {detail.review_note}</p>{detail.withdrawal_requested&&<p>Đã yêu cầu ngừng phân phối: {detail.withdrawal_reason}. Chưa xác nhận catalog mới.</p>}
 <details><summary>Gói ký số</summary><pre>{JSON.stringify(detail.bundle,null,2)}</pre></details>
 <label>Biên bản hoặc lý do<textarea maxLength={2000} rows={3} value={note} disabled={busy} onChange={e=>setNote(e.target.value)}/></label>
 {review&&!detail.own&&detail.status==="pending"&&<div className="publisher-controls"><button disabled={busy||!note.trim()||detail.parent_status!=="approved"} onClick={()=>decide("approved")}>Duyệt v2</button><button disabled={busy||!note.trim()} onClick={()=>decide("rejected")}>Từ chối</button></div>}
 {review&&!detail.own&&detail.status==="approved"&&!detail.withdrawal_requested&&<button disabled={busy} onClick={exportBundle}>Xuất gói ký ngoại tuyến</button>}
 {((detail.own&&detail.status==="pending")||((detail.own||review)&&detail.status==="approved"&&!detail.withdrawal_requested))&&<button className="publisher-secondary" disabled={busy||!note.trim()} onClick={withdraw}>{detail.status==="pending"?"Rút hồ sơ chờ":"Yêu cầu thu hồi bản phát hành"}</button>}
 </article>}{message&&<p role="status">{message}</p>}</section>
}
