/** No persistent credentials, approvals, command payloads or results. */
export const modeAllows=(mode,method)=>method.startsWith("cad_")?mode==="cad":method.startsWith("map_")?mode==="gis":method.startsWith("gis_")&&["gis","3d"].includes(mode);
export function validSession(s){
 return !!s&&typeof s.session_id==="string"&&/^[A-Za-z0-9_-]{1,128}$/.test(s.session_id)&&typeof s.browser_token==="string"&&s.browser_token.length>0&&s.browser_token.length<=256&&Number.isFinite(s.expires_in)&&s.expires_in>0&&s.expires_in<=900;
}
export function pinCommand(c,scope,now){
 if(!c||typeof c.id!=="string"||!/^[A-Za-z0-9_-]{1,128}$/.test(c.id)||typeof c.method!=="string"||c.method.length>128||!modeAllows(scope.mode,c.method)||!Number.isFinite(c.expires_in)||c.expires_in<=2||c.expires_in>900||!c.params||typeof c.params!=="object"||Array.isArray(c.params)||new TextEncoder().encode(JSON.stringify(c.params)).length>100000)throw Error("Lệnh không hợp lệ hoặc sai workspace.");
 return {...c,scope,deadline:now+c.expires_in*1000};
}
export function assertCommandCurrent(c,scope,now,minSeconds=0){
 if(!c||c.scope.mode!==scope.mode||c.scope.epoch!==scope.epoch||c.scope.cad!==scope.cad||!modeAllows(scope.mode,c.method)||now+minSeconds*1000>=c.deadline)throw Error("Workspace, bản vẽ hoặc thời hạn lệnh đã thay đổi.");
}
export function recordHistory(history,entry){
 return [{id:entry.id,method:entry.method,mode:entry.mode,status:entry.status,at:entry.at},...history.filter(x=>x.id!==entry.id)].slice(0,20);
}
