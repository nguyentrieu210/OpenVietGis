import assert from 'node:assert/strict';
import {parseHouseQrFocus,captureHouseQrFocus,readHouseQrFocus,clearHouseQrFocus} from './src/HouseQrLink.ts';
const v=new Map();
globalThis.sessionStorage={getItem:k=>v.get(k)??null,setItem:(k,x)=>v.set(k,x),removeItem:k=>v.delete(k)};
const org='11111111-1111-4111-8111-111111111111',project='22222222-2222-4222-8222-222222222222',source='33333333-3333-4333-8333-333333333333';
const query=new URLSearchParams({hn_org:org,hn_project:project,hn_source:source,hn_feature:'Khảo sát nhà 9 & 10'}).toString();
const parsed=parseHouseQrFocus('?'+query);
assert.deepEqual(parsed,{organizationId:org,projectId:project,sourceId:source,featureId:'Khảo sát nhà 9 & 10'});
assert.deepEqual(captureHouseQrFocus('?'+query),parsed);
assert.deepEqual(readHouseQrFocus(),parsed);
assert.equal(parseHouseQrFocus('?hn_org=%2F%2Fexample.com&'+query),null); // malformed first duplicate must be rejected.
clearHouseQrFocus();assert.equal(readHouseQrFocus(),null);
assert.equal(parseHouseQrFocus('?hn_org=invalid&hn_project='+project+'&hn_source='+source+'&hn_feature=test'),null);
console.log('PASS house QR canonical link parser, login persistence and scope validation');
