import {loadCsrfToken,type ApiClient} from "@openvietgis/client-sdk";
type P=Record<string,unknown>;
type Saved={id:string;name:string;revision:number;document?:unknown;graph?:{version:number;nodes:Node[]}};
type Ref={type:"step"|"layer";id:string};
type Node={id:string;algorithm:string;name:string;parameters:P;a:Ref;b?:Ref;c?:Ref};
type Host={client:ApiClient;org:string;pid:string;guard:(minSeconds?:number)=>void;refresh:()=>void;renderLayout?:(document:unknown,format:"pdf"|"png",guard:()=>void)=>Promise<{bytes:number;filename:string}>};
const vector=new Set(["buffer","clip","dissolve"]);
export const advancedWrites=new Set(["gis_raster_process","gis_processing_run","gis_model_save","gis_model_run","gis_layout_save","gis_plugin_activate","gis_plugin_disable","gis_plugin_create","gis_plugin_update"]);
export const advancedMethods=new Set([...advancedWrites,"gis_raster_layers","gis_raster_report","gis_raster_download","gis_processing_job","gis_models","gis_layouts","gis_layout_export","gis_plugins","gis_plugin_records"]);
const uuid=(v:unknown)=>{if(typeof v!=="string"||!/^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i.test(v))throw Error("Invalid ID");return v};
const key=(v:unknown)=>{if(typeof v!=="string"||!/^[a-zA-Z0-9_.-]{1,128}$/.test(v))throw Error("Invalid extension key");return v};
function download(blob:Blob,filename:string,guard:()=>void){guard();const u=URL.createObjectURL(blob),a=document.createElement("a");a.href=u;a.download=filename;a.click();setTimeout(()=>URL.revokeObjectURL(u),2000);return {download:filename,bytes:blob.size}}
function form(p:P){const f=new FormData();for(const [k,v] of Object.entries(p))if(v!==undefined&&v!==null)f.set(k,typeof v==="object"?JSON.stringify(v):String(v));return f}
export async function executeAdvanced(h:Host,method:string,p:P){
 const base="/organizations/"+encodeURIComponent(h.org)+"/projects/"+encodeURIComponent(h.pid),org="/organizations/"+encodeURIComponent(h.org);
 const get=<T,>(path:string)=>h.client.request<T>(base+path);
 const write=async(path:string,data:P|FormData,http="POST")=>{await loadCsrfToken(h.client);h.guard(path==="/raster/layers"||path==="/raster/reports"||path==="/processing/jobs"?75:0);return h.client.request<P>(base+path,{method:http,body:data instanceof FormData?data:JSON.stringify(data)})};
 const rasters=()=>get<{items:P[]}>("/raster/layers");
 const saved=async(path:string,id:unknown,revision:unknown)=>{const r=await get<{items:Saved[]}>(path),m=r.items.find(x=>x.id===uuid(id));if(!m||m.revision!==revision)throw Error("Revision changed; reload catalog");return m};
 const save=async(path:string,field:"document"|"graph")=>{const data={name:p.name,[field]:p[field],...(p.id?{revision:p.revision}:{})};return write(path+(p.id?"/"+uuid(p.id):""),data,p.id?"PUT":"POST")};
 const raster=async(args:P)=>{const allowed=["method","name","source_id","second_id","third_id","band_a","band_b","band_c","formula","clip_geojson","factor","resampling"];return write("/raster/layers",form(Object.fromEntries(allowed.filter(k=>k in args).map(k=>[k,args[k]]))))};
 const process=async(args:P)=>write("/processing/jobs",{...args,execute_now:true});
 switch(method){
 case "gis_raster_layers":return rasters();
 case "gis_raster_process":{const r=await raster(p);h.refresh();return r}
 case "gis_raster_report":{
   const {zones,...rest}=p,r=await write("/raster/reports",form({...rest,...(zones?{zones_json:zones}:{})}));
   if(new TextEncoder().encode(JSON.stringify(r)).length>75000)return {...download(new Blob([JSON.stringify(r)],{type:"application/json"}),"raster-report.json",h.guard),message:"Large result downloaded to GIS browser; no truncated data sent"};
   return r;
 }
 case "gis_raster_download":{const response=await h.client.requestRaw(base+"/raster/layers/"+uuid(p.layer_id)+"/file");return download(await response.blob(),"raster.tif",h.guard)}
 case "gis_processing_run":{const result=await process(p);h.refresh();return result}
 case "gis_processing_job":return get("/processing/jobs/"+uuid(p.job_id));
 case "gis_models":return get("/processing/models");
 case "gis_model_save":return save("/processing/models","graph");
 case "gis_model_run":{
   const m=await saved("/processing/models",p.model_id,p.revision),run=uuid(p.run_id);
   if(!m.graph||m.graph.version!==1||!m.graph.nodes.length||m.graph.nodes.length>12)throw Error("Invalid saved model");
   const nodes=m.graph.nodes, outputs=new Map<string,string>(), pending=new Map(nodes.map(n=>[n.id,n])), receipts:P[]=[];
   if(pending.size!==nodes.length)throw Error("Duplicate model steps");
   const resolve=(r?:Ref)=>r?(r.type==="layer"?uuid(r.id):outputs.get(r.id)):undefined;
   try{
     while(pending.size){
       const n=[...pending.values()].find(n=>[n.a,n.b,n.c].every(r=>!r||r.type==="layer"||outputs.has(r.id)));
       if(!n)throw Error("Invalid dependency graph");
       h.guard();
       const a=resolve(n.a),b=resolve(n.b),c=resolve(n.c),name=n.name.slice(0,100)+" · "+run.slice(0,8);
       if(!a)throw Error("Missing source");
       let r:P,id:unknown;
       if(vector.has(n.algorithm)){
         r=await process({algorithm_id:n.algorithm,input_layer_id:a,overlay_layer_id:b??null,parameters:n.parameters,output_name:name,operation_id:"model-"+run+"-"+n.id});
         id=r.output_layer_id;if(r.status!=="succeeded"||!id)throw Error("Vector step failed");
       }else{
         // Raster writes are not idempotent; never overwrite or duplicate this run's named output.
         const existing=await rasters();if(existing.items.some(x=>x.name===name))throw Error("Raster output already exists; inspect previous run before retrying");
         r=await raster({method:n.algorithm,name,source_id:a,second_id:b,third_id:c,...n.parameters});id=r.id;
       }
       outputs.set(n.id,uuid(id));receipts.push({step_id:n.id,layer_id:id,name,algorithm:n.algorithm});pending.delete(n.id);
     }
     return {status:"succeeded",model_id:m.id,run_id:run,outputs:receipts};
   }catch(e){return {status:"partial_or_failed",model_id:m.id,run_id:run,outputs:receipts,remaining:[...pending.keys()],error:e instanceof Error?e.message:"Model interrupted",warning:"Check persisted project layers before retrying. Completed outputs are retained."}}
   finally{if(receipts.length)h.refresh()}
 }
 case "gis_layouts":return get("/cartography/designer");
 case "gis_layout_save":return save("/cartography/designer","document");
 case "gis_layout_export":{
   const m=await saved("/cartography/designer",p.layout_id,p.revision);
   if(p.format==="json")return download(new Blob([JSON.stringify({name:m.name,document:m.document},null,2)],{type:"application/json"}),"layout.json",h.guard);
   if(!h.renderLayout||!(p.format==="pdf"||p.format==="png"))throw Error("Layout rendering unavailable");
   h.guard(135);return h.renderLayout(m.document,p.format,h.guard);
 }
 case "gis_plugins":{
   const activations=await get("/extensions/activations");
   const manifests=await h.client.request(org+"/extensions/manifests");return {activations,manifests};
 }
 case "gis_plugin_activate":return write("/extensions/activations",{manifest_id:uuid(p.manifest_id)});
 case "gis_plugin_disable":return write("/extensions/"+key(p.extension_key)+"/disable",{});
 case "gis_plugin_records":return get("/extensions/"+key(p.extension_key)+"/records"+(p.entity_name?"?entity_name="+encodeURIComponent(key(p.entity_name)):""));
 case "gis_plugin_create":{const {extension_key,...rest}=p;return write("/extensions/"+key(extension_key)+"/records",rest)}
 case "gis_plugin_update":{const {extension_key,record_id,...rest}=p;return write("/extensions/"+key(extension_key)+"/records/"+uuid(record_id),rest,"PUT")}
 default:throw Error("Unsupported advanced tool");
 }
}
