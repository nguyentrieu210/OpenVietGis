// Host-side parsing of the server-qualified SPEC075 read-only wire format.
// This is not signature verification and cannot authorize an execution.
import type {PluginV2Document,PluginV2Runtime} from "./external-plugins";
type Obj=Record<string,unknown>;
const bad=():never=>{throw new Error("INVALID_PLUGIN_V2_CONTRACT")};
const obj=(v:unknown):Obj=>v!==null&&typeof v==="object"&&!Array.isArray(v)?v as Obj:bad();
const exact=(v:Obj,keys:string[])=>{if(Object.keys(v).length!==keys.length||keys.some(k=>!Object.hasOwn(v,k)))bad()};
const str=(v:unknown,max:number):string=>typeof v==="string"&&[...v].length<=max?v:bad();
const token=(v:unknown)=>{const s=str(v,64);return /^[A-Za-z0-9_.-]+$/.test(s)?s:bad()};
const uuid=(v:unknown)=>{const s=str(v,36);return /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/.test(s)&&s!=="00000000-0000-0000-0000-000000000000"?s:bad()};
const hex=(v:unknown)=>{const s=str(v,64);return /^[0-9a-f]{64}$/.test(s)?s:bad()};
const arr=(v:unknown,max:number):unknown[]=>Array.isArray(v)&&v.length<=max?v:bad();
const integer=(v:unknown,min:number,max:number):number=>typeof v==="number"&&Number.isSafeInteger(v)&&v>=min&&v<=max?v:bad();
const policy:Record<string,string>={host_context:"host_context",host_notice:"host_notice",gis_layers:"map_view",gis_layer:"map_view"};
export function parseReadOnlyPluginDocument(value:unknown,permissions:readonly string[],extensionKey:string):PluginV2Document{
 if(new TextEncoder().encode(JSON.stringify(value)).length>256*1024)bad();
 const d=obj(value);exact(d,["slot","widgets","actions"]);
 if(!["workspace","map_tool","form","dashboard"].includes(str(d.slot,32)))bad();
 const ids=new Set<string>(),fields=new Set<string>(),actions=new Set<string>();
 const allowed=new Set(Object.values(policy).map(p=>"ext."+extensionKey+"."+p));
 if(permissions.some(p=>!allowed.has(p)))bad();
 for(const item of arr(d.actions,32)){
  const a=obj(item);exact(a,["id","command_id","permission","consent","timeout_ms","input"]);
  const id=token(a.id),command=token(a.command_id);
  if(actions.has(id)||!Object.hasOwn(policy,command)||a.consent!=="read"||a.permission!=="ext."+extensionKey+"."+policy[command]||!permissions.includes(a.permission as string))bad();
  actions.add(id);integer(a.timeout_ms,1,30000);
  const input=obj(a.input),keys=Object.keys(input);
  const expected:Record<string,boolean>={host_context:false,host_notice:true,gis_layer:true,gis_layers:false};
  const field=command==="host_notice"?"text":command==="gis_layer"?"layer_id":command==="gis_layers"?"after_id":null;
  if(keys.length===0){if(expected[command])bad()}
  else{
   if(field===null||keys.length!==1||keys[0]!==field)bad();
   const f=obj(input[keys[0]!]);exact(f,["kind","required"]);
   if(f.kind!=="text"||f.required!==expected[command])bad();
   fields.add(token(keys[0]));
  }
 }
 let count=0;
 const walk=(nodes:unknown,depth:number)=>{
  if(depth>16)bad();
  for(const value of arr(nodes,500)){
   const n=obj(value),id=token(n.id);
   if(++count>500||ids.has(id))bad();ids.add(id);
   switch(n.kind){
    case "text":exact(n,["kind","id","text"]);str(n.text,2000);break;
    case "input":exact(n,["kind","id","label","field"]);str(n.label,200);if(!fields.has(token(n.field)))bad();break;
    case "button":exact(n,["kind","id","label","action"]);str(n.label,200);if(!actions.has(token(n.action)))bad();break;
    case "group":exact(n,["kind","id","children"]);walk(n.children,depth+1);break;
    default:bad();
   }
  }
 };
 walk(d.widgets,1);
 return value as PluginV2Document;
}
export function parsePluginV2Runtime(value:unknown,expectedManifest:string):PluginV2Runtime{
 const r=obj(value);
 exact(r,["document","release_digest","activation_revision","manifest_id","profile","granted","grant_revision","permissions","dependency_lock","lock_digest","catalog_epoch"]);
 if(uuid(r.manifest_id)!==expectedManifest||r.profile!=="web_read_only"||typeof r.granted!=="boolean")bad();
 hex(r.release_digest);hex(r.lock_digest);integer(r.activation_revision,1,Number.MAX_SAFE_INTEGER);integer(r.catalog_epoch,1,Number.MAX_SAFE_INTEGER);
 if(!/^(0|[1-9][0-9]{0,18})$/.test(str(r.grant_revision,19)))bad();
 const pins=arr(r.dependency_lock,100),seen=new Set<string>();
 if(!pins.length)bad();
 for(const v of pins){
  const p=obj(v);exact(p,["key","version","release_digest","manifest_id","activation_id","activation_revision"]);
  const key=str(p.key,128);
  if(!/^[A-Za-z0-9_.-]+$/.test(key)||seen.has(key))bad();seen.add(key);
  str(p.version,64);hex(p.release_digest);uuid(p.manifest_id);uuid(p.activation_id);integer(p.activation_revision,1,Number.MAX_SAFE_INTEGER);
 }
 const root=obj(pins[pins.length-1]);
 if(root.manifest_id!==r.manifest_id||root.release_digest!==r.release_digest||root.activation_revision!==r.activation_revision)bad();
 const permissions=arr(r.permissions,64).map(p=>str(p,128));
 if(new Set(permissions).size!==permissions.length)bad();
 parseReadOnlyPluginDocument(r.document,permissions,root.key as string);
 return r as unknown as PluginV2Runtime;
}
