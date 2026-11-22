import type {SpatialMode,WorkspaceSession,ModeCapability} from "./session";
import "./mode-switch.css";
const modes:readonly {mode:SpatialMode;label:string;description:string}[]=[
 {mode:"gis",label:"GIS 2D",description:"Bản đồ và dữ liệu không gian"},
 {mode:"cad",label:"CAD",description:"Bản vẽ kỹ thuật"},
 {mode:"3d",label:"3D",description:"Mô hình không gian ba chiều"},
];
export function SpatialModeSwitch({session,capabilities,onSelect}:{session:WorkspaceSession;capabilities:Record<SpatialMode,ModeCapability>;onSelect:(mode:SpatialMode)=>void}){
 return <div className="ovg-modebar">
  <div className="ovg-modebar-group" role="group" aria-label="Chế độ không gian">
   {modes.map(({mode,label,description})=>{
    const capability=capabilities[mode],active=session.active===mode,loading=session.pending?.mode===mode;
    const reason=capability.available?description:capability.reason;
    return <button key={mode} type="button" aria-pressed={active} aria-label={capability.available?label:label+" · "+reason} title={reason}
      aria-disabled={!capability.available&&!active} data-loading={loading?"true":undefined} onClick={()=>{if(capability.available||active)onSelect(mode)}}>
      {label}{loading&&<span aria-hidden="true" className="ovg-modebar-dot"/>}
     </button>;
   })}
  </div>
  {session.error&&<span className="ovg-modebar-status" role="status">{session.error}</span>}
  {!session.error&&session.pending&&<span className="ovg-modebar-status" role="status">Đang mở chế độ…</span>}
 </div>;
}
