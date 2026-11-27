import type {WorkbenchId} from "./workbench-contract";
const tools:Record<string,WorkbenchId>={spatial:"spatial",transfers:"transfers",editing:"editing",queries:"queries",processing:"processing",cartography:"cartography","platform-hub":"ops",cad:"cad",external:"external",collab:"collab"};
export function readWorkspaceRoute(path:string){
 const clean=path.replace(/^\/devui(?=\/)/,"");
 const match=/^\/(?:app\/)?organizations\/([^/]+)(?:\/projects\/([^/]+)(?:\/([^/]+))?)?(?:\/|$)/.exec(clean);
 if(!match)return {org:"",project:"",tool:null as WorkbenchId|null,account:clean==="/identity"||clean==="/app/account"};
 try{return {org:decodeURIComponent(match[1]!),project:match[2]?decodeURIComponent(match[2]):"",tool:Object.hasOwn(tools,match[3]??"")?tools[match[3]!]!:null,account:false}}catch{return {org:"",project:"",tool:null,account:false}}
}
export const initialWorkspaceRoute=readWorkspaceRoute(typeof location==="undefined"?"/":location.pathname);
