/** Authenticated, one-time, project-scoped GIS focus from a printed house QR. */
export type HouseQrFocus = {organizationId:string; projectId:string; sourceId:string; featureId:string};
const KEY="ovg:house-qr-focus:v1";
const TTL_MS=20*60*1000;
const UUID=/^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i;

export function parseHouseQrFocus(query:string):HouseQrFocus|null {
  const p=new URLSearchParams(query);
  const org=p.get("hn_org")??"",project=p.get("hn_project")??"",source=p.get("hn_source")??"",feature=p.get("hn_feature")??"";
  if(!UUID.test(org)||!UUID.test(project)||!UUID.test(source)||!feature.trim()||feature.length>128||/[\u0000-\u001f]/.test(feature))return null;
  return {organizationId:org,projectId:project,sourceId:source,featureId:feature};
}

export function captureHouseQrFocus(query:string):HouseQrFocus|null {
  const focus=parseHouseQrFocus(query);
  if(focus){try{sessionStorage.setItem(KEY,JSON.stringify({focus,created:Date.now()}));}catch{/* no storage */}}
  return focus;
}

export function readHouseQrFocus():HouseQrFocus|null {
  try{
    const raw=sessionStorage.getItem(KEY);
    if(!raw)return null;
    const item=JSON.parse(raw) as {focus:HouseQrFocus;created:number};
    if(!Number.isFinite(item.created)||item.created>Date.now()+60000||Date.now()-item.created>TTL_MS){sessionStorage.removeItem(KEY);return null}
    const v=item.focus;
    if(!v||!UUID.test(v.organizationId)||!UUID.test(v.projectId)||!UUID.test(v.sourceId)||typeof v.featureId!=="string"||!v.featureId.trim()||v.featureId.length>128)return null;
    return v;
  }catch{return null;}
}

export function clearHouseQrFocus():void {
  try{sessionStorage.removeItem(KEY)}catch{/* ignore */}
}
