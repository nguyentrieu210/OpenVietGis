import {useEffect,useState} from "react";
import {createRoot} from "react-dom/client";
import {ApiClient} from "../../../client-sdk/src/index";
import {CadEditorPanel,type CadFileVersion} from "../../../../apps/web-next/src/CadEditorPanel";
const client=new ApiClient({baseUrl:"/fixture"}),payload=new TextEncoder().encode("0\nSECTION\n2\nENTITIES\n0\nENDSEC\n0\nEOF\n");
function Fixture(){
 const [doc,setDoc]=useState<CadFileVersion|null>(null),[closed,setClosed]=useState(false),[action,setAction]=useState(""),[canEdit,setCanEdit]=useState(true);
 useEffect(()=>{void crypto.subtle.digest("SHA-256",payload).then(bytes=>setDoc({id:"testdoc",filename:"test.dxf",format:"dxf",version:2,byte_size:payload.byteLength,sha256:Array.from(new Uint8Array(bytes),b=>b.toString(16).padStart(2,"0")).join(""),created_at:"2026-11-23T00:00:00Z"}))},[]);
 return <main style={{height:"95vh",display:"flex",flexDirection:"column"}}><button onClick={()=>setCanEdit(false)}>Revoke</button><p id="action">{action}</p>{!doc?<p>Loading</p>:closed?<p id="closed">Closed</p>:<CadEditorPanel client={client} path="/documents" document={doc} origin="http://127.0.0.1:4281" writable={true} canEdit={canEdit} focusHandle="A1" expire={()=>setAction("expired")} close={()=>setClosed(true)} onNavigate={(link,mode)=>setAction(mode+":"+link.handle+":"+link.feature_id)}/>}</main>;
}
createRoot(document.querySelector("#root")!).render(<Fixture/>);
