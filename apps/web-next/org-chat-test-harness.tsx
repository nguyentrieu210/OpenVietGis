import React,{useState} from "react";
import {createRoot} from "react-dom/client";
import {ApiClient} from "@openvietgis/client-sdk";
import {OrgChat,createOrgDraftStore} from "./src/OrgChat";
const client=new ApiClient({baseUrl:"/api/v1"});
function Harness(){const [open,setOpen]=useState(true),[drafts]=useState(createOrgDraftStore);return <>{open?<OrgChat client={client} org="org-test" accountId="me" drafts={drafts} close={()=>setOpen(false)}/>:<button onClick={()=>setOpen(true)}>Mở lại chat</button>}</>}
createRoot(document.getElementById("root")!).render(<Harness/>);
