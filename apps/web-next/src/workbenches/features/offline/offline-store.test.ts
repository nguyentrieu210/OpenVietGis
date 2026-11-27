import { describe, expect, it } from "vitest";
import type { OfflinePackage } from "@openvietgis/client-sdk";
import { makeRecord, normalizeField, queueDraft, sha256, verifyRecord, wireDrafts,
  type Draft, type Field, type Snapshot } from "./offline-store";

function sample() {
  const expires_at = new Date(Date.now()+3600000).toISOString();
  const snapshot: Snapshot = {
    version:1,package_id:"package",actor_id:"actor",organization_id:"org",project_id:"prj",
    layer_id:"layer",layer_name:"Sample",source_id:"source",content_revision:"content",schema_revision:"schema",
    crs_code:4326,geometry_family:"Point",aoi_bbox:{min_x:105,min_y:20,max_x:106,max_y:21},
    created_at:new Date().toISOString(),expires_at,
    fields:[{field_id:"size",name:"Size",type:"decimal",nullable:false}],
    features:[{feature_id:"p1",properties:{size:"123.123456789012345678"},geometry:{type:"Point",coordinates:[105.5,20.5]}}]
  };
  return snapshot;
}
async function fixture() {
  const snapshot = sample();
  const bytes = new TextEncoder().encode(JSON.stringify(snapshot)).buffer;
  const gpkgBytes = new TextEncoder().encode("SQLite format 3\0synthetic test bytes").buffer;
  const pkg: OfflinePackage = {id:"package",name:"Sample",profile:"standard_pwa",crs_code:4326,
    feature_count:1,sha256:await sha256(gpkgBytes),byte_size:gpkgBytes.byteLength,
    revoked_at:null,expires_at:snapshot.expires_at,
    layer_pins:[{layer_id:snapshot.layer_id,source_id:snapshot.source_id,content_revision:snapshot.content_revision,
      schema_revision:snapshot.schema_revision,snapshot_sha256:await sha256(bytes)}]};
  const record = await makeRecord(pkg, bytes, gpkgBytes,"actor","org","prj");
  return {pkg,snapshot,record,bytes,gpkgBytes};
}

describe("verified offline package",()=>{
  it("requires complete, scope-matched projection and GeoPackage checksums",async()=>{
    const {record,snapshot,pkg,bytes,gpkgBytes}=await fixture();
    expect(await verifyRecord(record)).toEqual(snapshot);
    await expect(makeRecord(pkg,bytes,gpkgBytes,"other","org","prj")).rejects.toThrow(/khớp/);
    await expect(makeRecord(pkg,new ArrayBuffer(0),gpkgBytes,"actor","org","prj")).rejects.toThrow(/hỏng/);
    await expect(verifyRecord({...record,gpkgBytes:new ArrayBuffer(0)})).rejects.toThrow(/hỏng/);
    await expect(verifyRecord({...record,locked:true})).rejects.toThrow(/khóa/);
    await expect(verifyRecord({...record,expiresAt:"2020-01-01T00:00:00Z"})).rejects.toThrow(/hết hạn/);
  });
  it("queues a stable group and prohibits intent mutation after attempted submission",async()=>{
    const {record}=await fixture();
    const d: Draft={op_type:"update",feature_id:"p1",properties:{size:"12.5"}};
    const queued=queueDraft(record,d);
    const second=queueDraft(queued,{...d,properties:{size:"13"}});
    expect(second.batch?.id).toBe(queued.batch?.id);
    expect(second.drafts).toHaveLength(1);
    expect(second.drafts[0]?.properties).toEqual({size:"13"});
    expect(()=>queueDraft({...second,batch:{id:second.batch?.id??"",attempted:true}},d)).toThrow(/không thể/);
    const id = queued.drafts[0]?.client_operation_id;
    expect(id).toMatch(/^[0-9a-f-]{36}$/);
    expect(second.drafts[0]?.client_operation_id).toBe(id);
    expect(wireDrafts(second.drafts)).toEqual([{op_type:"update",feature_id:"p1",properties:{size:"13"},client_operation_id:id}]);
    const distinct=queueDraft(second,{...d,feature_id:"p2"});
    expect(distinct.drafts[1]?.client_operation_id).not.toBe(id);
    // Persisted legacy, already-attempted wire intent must not acquire a new ID.
    expect(wireDrafts([d])).toEqual([d]);
    expect(wireDrafts([{...d, client_operation_id:""}])[0]?.client_operation_id).toBe("");
  });
});

describe("typed local draft values",()=>{
  const field: Field={field_id:"v",name:"v",type:"decimal",nullable:false};
  it("normalizes decimal precision without floating point coercion",()=>{
    expect(normalizeField(field,"001.123456789012345678",false)).toBe("1.123456789012345678");
    expect(normalizeField(field,"-0.000",false)).toBe("0");
    expect(()=>normalizeField(field,"12e3",false)).toThrow();
    expect(()=>normalizeField(field,"0.1234567890123456789",false)).toThrow();
  });
  it("enforces type/nullability",()=>{
    expect(()=>normalizeField(field,"",true)).toThrow();
    expect(normalizeField({...field, nullable:true},"",true)).toBeNull();
    expect(()=>normalizeField({...field,type:"integer"},"2147483648",false)).toThrow();
    expect(normalizeField({...field,type:"boolean"},"false",false)).toBe(false);
    expect(normalizeField({...field,type:"timestamp"},"2026-11-21T00:00:00.123Z",false)).toBe("2026-11-21T00:00:00.123000Z");
  });
});
