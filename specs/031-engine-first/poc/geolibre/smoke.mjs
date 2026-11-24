import assert from "node:assert/strict";
import {readFile} from "node:fs/promises";
import {createHash} from "node:crypto";
import {performance} from "node:perf_hooks";
import {initSync,version,vector_formats,vector_to_geojson,vector_to_geojson_reproject} from "geolibre-wasm";
const root=new URL("./node_modules/geolibre-wasm/",import.meta.url);
const bytes=await readFile(new URL("geolibre_wasm_bg.wasm",root));
const cli=await readFile(new URL("geolibre-cli.wasm",root));
const sha256=b=>createHash("sha256").update(b).digest("hex");
assert.equal(sha256(bytes),"f2f06219a1a1477448bcea619629723631a01e3b82b4385e9695ef5189be212f");
const t=performance.now();initSync({module:bytes});const initMs=performance.now()-t;
assert.equal(version(),"1.8.4");
const fixture={type:"FeatureCollection",features:[
 {type:"Feature",id:"fixture-001",properties:{name:"Điểm A",number:42,flag:true,empty:null},geometry:{type:"Point",coordinates:[106.9,11.5]}},
 {type:"Feature",id:"fixture-002",properties:{name:"Vùng B",number:0,flag:false,empty:null},geometry:{type:"Polygon",coordinates:[[[106.9,11.5],[106.91,11.5],[106.91,11.51],[106.9,11.5]]]}}
]};
const input=new TextEncoder().encode(JSON.stringify(fixture));
const out=JSON.parse(vector_to_geojson(input,"geojson"));
assert.equal(out.features.length,fixture.features.length);
for(let i=0;i<fixture.features.length;i++){
 assert.deepEqual(out.features[i].geometry,fixture.features[i].geometry);
 assert.deepEqual(out.features[i].properties,fixture.features[i].properties);
}
const idsPreserved=out.features.every((f,i)=>f.id===fixture.features[i].id);
const projected=JSON.parse(vector_to_geojson_reproject(input,"geojson",3857,4326));
const rad=Math.PI/180,radius=6378137;
const [lon,lat]=fixture.features[0].geometry.coordinates;
const expected=[radius*lon*rad,radius*Math.log(Math.tan(Math.PI/4+lat*rad/2))];
const actual=projected.features[0].geometry.coordinates;
const errorM=Math.hypot(actual[0]-expected[0],actual[1]-expected[1]);
assert.ok(errorM<0.001);
const large={type:"FeatureCollection",features:Array.from({length:10000},(_,i)=>({type:"Feature",id:"id-"+i,geometry:{type:"Point",coordinates:[106.9+(i%100)*0.00001,11.5+Math.floor(i/100)*0.00001]},properties:{ordinal:i}}))};
const largeInput=new TextEncoder().encode(JSON.stringify(large));
const durations=[];
for(let i=0;i<30;i++){
 const start=performance.now();const result=JSON.parse(vector_to_geojson(largeInput,"geojson"));
 durations.push(performance.now()-start);
 assert.equal(result.features.length,10000);
 assert.equal(result.features[9999].properties.ordinal,9999);
}
durations.sort((a,b)=>a-b);
console.log(JSON.stringify({status:"research-only",version:version(),formats:vector_formats(),libraryBytes:bytes.length,cliBytes:cli.length,librarySha256:sha256(bytes),cliSha256:sha256(cli),initMs,geometryAndAttributesPreserved:true,idsPreserved,authoritativeWriteAllowed:false,mercatorControlErrorM:errorM,features:10000,samples:30,p50Ms:durations[14],p95Ms:durations[28],inputBytes:largeInput.length,node:process.version},null,2));
