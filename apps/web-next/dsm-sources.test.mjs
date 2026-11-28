import assert from "node:assert/strict";
import test from "node:test";
import { csvToGeoJSON, guessDelim, guessXY, splitCsv, remoteLimit, validateRemoteCollection, wfsFetch, arcgisFetch } from "./src/dsm-sources.ts";

test("csv with quotes, semicolons and decimal commas becomes points", () => {
  const text = '﻿ten;kinh_do;vi_do;ghi_chu\n"Mốc A";106,7123;10,7001;"có ""ngoặc"""\nMốc B;;10.7;x\n';
  const d = guessDelim(text); assert.equal(d, ";");
  const h = splitCsv(text.replace(/^﻿/, ""), d)[0]; assert.deepEqual(guessXY(h), { x: "kinh_do", y: "vi_do" });
  const { fc, skipped } = csvToGeoJSON(text, { delim: d, x: "kinh_do", y: "vi_do" });
  assert.equal(skipped, 1);
  assert.deepEqual(fc.features[0].geometry, { type: "Point", coordinates: [106.7123, 10.7001] });
  assert.deepEqual(fc.features[0].properties, { ten: "Mốc A", ghi_chu: 'có "ngoặc"' });
});

test("CSV rejects ambiguous columns, bad rows and incomplete quotes",()=>{
 const opts={delim:";",x:"x",y:"y"};
 for(const input of ['x;y;name;name\n1;2;a;b','x;y;name\n1;2','x;y;__proto__\n1;2;x','x;y;name\n1;2;"broken'])assert.throws(()=>csvToGeoJSON(input,opts));
 assert.throws(()=>csvToGeoJSON("x;y\n1;2",{...opts,y:"x"}));
 assert.equal(guessDelim('"name,with,commas";x;y\nA;1;2'),";");
 const {fc,skipped}=csvToGeoJSON("x;y;code;large;precise\n1;2;001;9007199254740993;0.123456789012345678\n0x10;2;x;y;z",opts);
 assert.equal(skipped,1);assert.deepEqual(fc.features[0].properties,{code:"001",large:"9007199254740993",precise:"0.123456789012345678"});
});
test("remote collections reject invalid limits and malformed results",()=>{
 for(const n of [0,1.5,50001,NaN])assert.throws(()=>remoteLimit(n));
 const f={type:"Feature",geometry:null,properties:{}};
 assert.throws(()=>validateRemoteCollection({type:"FeatureCollection",features:[f,f]},1));
 assert.throws(()=>validateRemoteCollection({type:"FeatureCollection",features:[{...f,properties:[]}]},1));
 assert.equal(validateRemoteCollection({type:"FeatureCollection",features:[f]},1).features.length,1);
});
test("OGC and ArcGIS path construction preserves query parameters",async()=>{
 const original=globalThis.fetch,urls=[];
 globalThis.fetch=async(url)=>{urls.push(new URL(url));return new Response(JSON.stringify({type:"FeatureCollection",features:[]}),{status:200})};
 try{await wfsFetch("https://example.invalid/collections?lang=vi","roads",10);await arcgisFetch("https://example.invalid/FeatureServer/0?lang=vi",10);
 assert.equal(urls[0].pathname,"/collections/roads/items");assert.equal(urls[0].searchParams.get("lang"),"vi");
 assert.equal(urls[1].pathname,"/FeatureServer/0/query");assert.equal(urls[1].searchParams.get("lang"),"vi");
 }finally{globalThis.fetch=original}
});
