import {useState} from "react";
import {createRoot} from "react-dom/client";
import {SpatialModeSwitch} from "../../src/ModeSwitch";
import {createSession,requestMode,finishMode,type SpatialMode,type ModeCapability} from "../../src/session";
function Fixture(){
 const [session,setSession]=useState(()=>createSession({accountId:"test",organizationId:"test-org",projectId:"test-project"}));
 const [dirty,setDirty]=useState(false);
 const capabilities:Record<SpatialMode,ModeCapability>={gis:{available:true},cad:{available:true},"3d":{available:false,reason:"Chưa có engine 3D"}};
 function select(mode:SpatialMode){
  const next=requestMode(session,mode,capabilities[mode],{dirty,saving:false});setSession(next);
  if(next.pending){const token=next.pending;setTimeout(()=>setSession(old=>finishMode(old,token,{ready:true,capability:capabilities[mode],blockers:{dirty:false,saving:false}})),150);}
 }
 return <main><h1>Spatial mode test fixture</h1><SpatialModeSwitch session={session} capabilities={capabilities} onSelect={select}/>
 <label><input type="checkbox" checked={dirty} onChange={e=>setDirty(e.target.checked)}/>Chỉnh sửa chưa lưu</label>
 <p data-testid="active">Đang dùng: {session.active}</p><p data-testid="project">{session.scope.projectId}</p></main>;
}
createRoot(document.querySelector("#root")!).render(<Fixture/>);
