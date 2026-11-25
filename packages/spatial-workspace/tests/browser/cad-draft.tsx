import {useState} from "react";
import {createRoot} from "react-dom/client";
import {ApiClient} from "../../../client-sdk/src/index";
import {CadDraftExport,supportsCadDraft} from "../../../../apps/web-next/src/CadDraftExport";
import {CadDraftImport} from "../../../../apps/web-next/src/CadDraftImport";
const client=new ApiClient({baseUrl:"/fixture"}),doc={id:"testdoc",version:2,sha256:"a".repeat(64),filename:"test.dxf",format:"dxf" as const,byte_size:1,created_at:"2026-11-24"};
const link={document_id:doc.id,version:2,filename:doc.filename,source_id:"source",layer_id:"layer",feature_id:"feature",handle:"A1",layer_name:"Buildings",original_revision:false,conversion_id:"00000000-0000-4000-8000-000000000048",content_revision:"b".repeat(64),schema_revision:"c".repeat(64)};
function ImportFixture() {
 const [allow,setAllow]=useState(true);
 return <main><button onClick={()=>setAllow(false)}>Revoke</button>{allow&&<CadDraftImport
  client={client} path="/project/cad/documents" document={{...doc,version:3}}
  link={{...link,original_revision:true}} expire={()=>setAllow(false)}/>}</main>;
}
function Fixture(){const [allow,setAllow]=useState(true);return <main><button onClick={()=>setAllow(false)}>Revoke</button>{allow&&supportsCadDraft(link)&&<CadDraftExport client={client} path="/documents" document={doc} link={link} expire={()=>setAllow(false)}/>}</main>}
createRoot(window.document.querySelector("#root")!).render(new URLSearchParams(location.search).has("import")?<ImportFixture/>:<Fixture/>);
