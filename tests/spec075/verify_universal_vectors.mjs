import assert from 'node:assert/strict';
import {createHash,createPublicKey,verify} from 'node:crypto';
import {readFileSync} from 'node:fs';
const fixture=JSON.parse(readFileSync(new URL('../../fixtures/spec075/python-signed-catalog.json',import.meta.url),'utf8'));
const sorted=v=>Array.isArray(v)?v.map(sorted):v!==null&&typeof v==='object'?Object.fromEntries(Object.keys(v).sort().map(k=>[k,sorted(v[k])])):v;
const canonical=v=>Buffer.from(JSON.stringify(sorted(v)),'utf8');
const sha=b=>createHash('sha256').update(b).digest('hex');
const key=base64=>createPublicKey({key:Buffer.concat([Buffer.from('302a300506032b6570032100','hex'),Buffer.from(base64,'base64')]),format:'der',type:'spki'});
const check=(pub,v,sig)=>verify(null,canonical(v),pub,Buffer.from(sig,'base64'));
const op=key(fixture.operator_public_key);
assert(check(op,fixture.v1.payload,fixture.v1.signature));
const members=[];
for(const r of fixture.v2.releases){
 const e=r.envelope, publisher=fixture.v1.payload.publishers.find(p=>p.id===e.publisher_id);
 const legacy=fixture.v1.payload.releases.find(r=>r.manifest.extension_key===e.extension_key&&r.manifest.version===e.version);
 assert.equal(sha(canonical(legacy.manifest)),e.manifest_digest);
 assert.equal(sha(Buffer.from(publisher.public_key,'base64')),e.key_id);
 const pub=key(publisher.public_key);
 assert(check(pub,{publisher_id:legacy.publisher_id,manifest:legacy.manifest},legacy.signature));
 assert(check(pub,e,r.publisher_signature));
 assert.equal(sha(Buffer.from(r.contribution,'base64')),e.contribution_digest);
 const digest=sha(canonical(e));
 assert(check(op,{schema:'openvietgis.review.v2',envelope_digest:digest,epoch:fixture.v2.epoch},r.operator_signature));
 members.push({envelope_digest:digest,publisher_signature:r.publisher_signature,operator_signature:r.operator_signature});
}
const membership={schema:'openvietgis.marketplace.membership.v2',epoch:fixture.v2.epoch,members};
assert(check(op,membership,fixture.v2.catalog_signature));
assert(!check(op,{...membership,members:members.slice(1)},fixture.v2.catalog_signature));
console.log('SPEC075_NODE_SIGNATURE_VECTORS_PASS',members.length);
