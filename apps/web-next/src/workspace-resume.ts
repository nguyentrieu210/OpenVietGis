import type {SpatialMode,ProjectScope} from "@openvietgis/spatial-workspace/session";
export function resumeModeKey(scope:ProjectScope):string {
 return "ovg:resume-mode:v1:"+scope.accountId+":"+scope.organizationId+":"+scope.projectId;
}
/** Explicit GIS workbench/QR destinations take precedence over a saved mode.
 * A stored mode is only a UI preference, never authorization.
 */
export function resolveResumeMode(scope:ProjectScope, forceGis:boolean, read:(key:string)=>string|null):SpatialMode {
 if(forceGis)return "gis";
 try{const mode=read(resumeModeKey(scope));return mode==="cad"||mode==="3d"?mode:"gis"}catch{return "gis"}
}
