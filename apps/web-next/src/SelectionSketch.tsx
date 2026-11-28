import {useEffect,useState,type RefObject} from "react";
import type * as ml from "./map-surface";
import {selectionPolygon} from "./query-selection";
import {validXY,type XY} from "./digitizing";
export function SelectionSketch({map,finish,cancel}:{map:RefObject<ml.Map|null>;finish:(points:XY[])=>void;cancel:()=>void}){
 const [points,setPoints]=useState<XY[]>([]),[error,setError]=useState("");
 useEffect(()=>{const m=map.current;if(!m){cancel();return}const stop=()=>cancel();m.on("movestart",stop);m.on("resize",stop);const key=(e:KeyboardEvent)=>{if(e.key==="Escape"){e.preventDefault();e.stopPropagation();cancel()}};window.addEventListener("keydown",key,true);return()=>{m.off("movestart",stop);m.off("resize",stop);window.removeEventListener("keydown",key,true)}},[map,cancel]);
 const m=map.current;
 const screen=points.map(p=>m?.project(p)).filter(p=>p!==undefined).map(p=>p.x+","+p.y);
 return <div style={{position:"absolute",inset:0,zIndex:20,background:"transparent",cursor:"crosshair"}} aria-label="Vẽ vùng chọn đa giác">
 <svg style={{width:"100%",height:"100%",display:"block"}} onPointerDown={e=>{if(e.button!==0||!m||points.length>=256)return;const rect=m.getCanvas().getBoundingClientRect();const p=m.unproject([e.clientX-rect.left,e.clientY-rect.top]);const xy:XY=[p.lng,p.lat];if(!validXY(xy)){setError("Điểm ngoài phạm vi WGS84.");return}setPoints(old=>[...old,xy]);setError("")}}>
 {screen.length>=3?<polygon points={screen.join(" ")} fill="#0b8b8b" fillOpacity=".18" stroke="#007b80" strokeWidth="2"/>:<polyline points={screen.join(" ")} fill="none" stroke="#007b80" strokeWidth="2"/>}
 {screen.map((p,i)=>{const [x,y]=p.split(",").map(Number);return <circle key={i} cx={x} cy={y} r="4" fill="#007b80" stroke="white"/>})}
 </svg>
 <div className="panel" style={{position:"absolute",top:12,left:12,padding:10,cursor:"default"}}><strong>Vẽ vùng chọn · {points.length}/256 đỉnh</strong><p>Bấm từng đỉnh, sau đó xác nhận. Esc để hủy.</p><button disabled={!points.length} onClick={()=>{setPoints(old=>old.slice(0,-1));setError("")}}>Bỏ đỉnh cuối</button><button disabled={points.length<3} onClick={()=>{try{selectionPolygon(points);finish(points)}catch(e){setError(e instanceof Error?e.message:String(e))}}}>Xác nhận đa giác</button><button onClick={cancel}>Hủy</button>{error&&<p role="alert">{error}</p>}</div>
 </div>
}
