import {useCallback,useEffect,useRef,useState} from "react";
import {ApiClient,ApiError,loadCsrfToken} from "@openvietgis/client-sdk";

type Target={layer:{source_id:string};feature:{feature_id:string}};
type Claim={source_id:string;feature_id:string;token:string};
type Lease={source_id:string;feature_id:string;actor_id:string;actor_name:string;expires_at:string};
export type ClaimFeature=(target:Target,open:()=>void,failed?:()=>void)=>void;
export function useFeatureLeases(client:ApiClient,org:string,pid:string,enabled:boolean,notify:(s:string)=>void){
 const base="/organizations/"+org+"/projects/"+pid+"/collaboration/edit-locks";
 const [leases,setLeases]=useState<Lease[]>([]),[held,setHeld]=useState<Claim|null>(null),[pending,setPending]=useState(false);
 const current=useRef<Claim|null>(null),claiming=useRef(false),alive=useRef(true),generation=useRef(0),notice=useRef(notify);notice.current=notify;
 const refresh=useCallback(async()=>{const g=++generation.current;if(!enabled){setLeases([]);return}try{const result=await client.request<{items:Lease[]}>(base,{cache:"no-store"});if(alive.current&&g===generation.current)setLeases(result.items)}catch{if(alive.current&&g===generation.current)setLeases([])}},[client,base,enabled]);
 const post=useCallback(async(path:string,claim:Claim)=>{await loadCsrfToken(client);return client.request(base+path,{method:"POST",body:JSON.stringify(claim)})},[client,base]);
 const release=useCallback(()=>{const claim=current.current;if(!claim)return;current.current=null;setHeld(null);void post("/release",claim).then(refresh).catch(()=>{})},[post,refresh]);
 useEffect(()=>{alive.current=true;return()=>{alive.current=false;generation.current++;const claim=current.current;current.current=null;if(claim)void post("/release",claim).catch(()=>{})}},[post]);
 useEffect(()=>{void refresh()},[refresh]);
 useEffect(()=>{if(!enabled)release()},[enabled,release]);
 useEffect(()=>{if(!leases.length)return;const expiry=Math.min(...leases.map(x=>Date.parse(x.expires_at)));const timer=setTimeout(()=>setLeases(items=>items.filter(x=>Date.parse(x.expires_at)>Date.now())),Math.max(50,expiry-Date.now()+25));return()=>clearTimeout(timer)},[leases]);
 useEffect(()=>{if(!held)return;let stopped=false,renewing=false;const timer=setInterval(async()=>{if(stopped||renewing||current.current!==held)return;renewing=true;try{await post("",held);if(stopped||current.current!==held){void post("/release",held).catch(()=>{});return}void refresh()}catch{if(!stopped){notice.current("Mất khóa biên tập · giữ nguyên bản nháp; máy chủ vẫn kiểm tra xung đột khi lưu.")}}finally{renewing=false}},15000);return()=>{stopped=true;clearInterval(timer)}},[held,post,refresh]);
 const claim:ClaimFeature=(target,open,failed)=>{
  if(!enabled){open();return}if(claiming.current||current.current)return;
  const next:Claim={source_id:target.layer.source_id,feature_id:target.feature.feature_id,token:crypto.randomUUID()};
  claiming.current=true;setPending(true);
  void (async()=>{try{await post("",next);if(!alive.current){void post("/release",next).catch(()=>{});return}current.current=next;setHeld(next);void refresh();open()}catch(e){notice.current(e instanceof ApiError&&e.status===409?"Đối tượng đang được người khác hoặc phiên khác biên tập.": "Không thể nhận khóa biên tập. Vui lòng thử lại.");failed?.();void refresh()}finally{claiming.current=false;if(alive.current)setPending(false)}})();
 };
 return {leases,held,pending,claim,release,refresh};
}
