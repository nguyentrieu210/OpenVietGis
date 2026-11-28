/** IndexedDB is the authoritative local queue; per-row transactions avoid
 * replacing a whole queue with a stale snapshot from another browser tab. */
export type SurveyOperation={
 client_uuid:string;building_id:string;condition:string;queue_actor_id:string;expected_building_revision:number;
 x?:number;y?:number;crs_code?:number;gps_accuracy_m?:number;
 recorded_by_actor_id?:string;photo_artifact_ids:string[];
};
export type SurveyDraft={key:string;scope:string;operation:SurveyOperation;photo?:File;conflict?:string};
export type SyncReceipt={client_uuid:string;survey_id:string;status:"applied"|"replayed"};
export const scopeKey=(actor:string,org:string,project:string)=>JSON.stringify([actor,org,project]);
export const draftKey=(scope:string,id:string)=>JSON.stringify([scope,id]);
export function acknowledgedIds(sent:readonly SurveyOperation[],receipts:unknown,conflicts:readonly Record<string,unknown>[]):Set<string>{
 const result=new Set<string>();if(!Array.isArray(receipts))return result;
 const allowed=new Set(sent.map(x=>x.client_uuid)),blocked=new Set(conflicts.map(x=>x.client_uuid));
 for(const receipt of receipts){
  if(receipt&&allowed.has(receipt.client_uuid)&&!blocked.has(receipt.client_uuid)&&typeof receipt.survey_id==="string"&&receipt.survey_id.length>0&&(receipt.status==="applied"||receipt.status==="replayed"))result.add(receipt.client_uuid);
 }
 return result;
}
function openDb():Promise<IDBDatabase>{
 return new Promise((resolve,reject)=>{
  const req=indexedDB.open("ovg-hn-survey-outbox",1);
  req.onupgradeneeded=()=>{const store=req.result.createObjectStore("drafts",{keyPath:"key"});store.createIndex("scope","scope");};
  req.onsuccess=()=>resolve(req.result);req.onerror=()=>reject(req.error);req.onblocked=()=>reject(Error("Không mở được kho phiếu cục bộ"));
 });
}
async function transaction<T>(mode:IDBTransactionMode,work:(store:IDBObjectStore)=>IDBRequest<T>):Promise<T>{
 const db=await openDb();
 try{return await new Promise<T>((resolve,reject)=>{
  const tx=db.transaction("drafts",mode);const req=work(tx.objectStore("drafts"));
  tx.oncomplete=()=>resolve(req.result);tx.onabort=()=>reject(tx.error??Error("Không lưu được phiếu cục bộ"));tx.onerror=()=>reject(tx.error);
 });}finally{db.close()}
}
export const listDrafts=(scope:string)=>transaction("readonly",store=>store.index("scope").getAll(scope)) as Promise<SurveyDraft[]>;
export const addDraft=(row:SurveyDraft)=>transaction("readwrite",store=>store.add(row));
export const saveDraft=(row:SurveyDraft)=>transaction("readwrite",store=>store.put(row));
export const deleteDraft=(key:string)=>transaction("readwrite",store=>store.delete(key));
