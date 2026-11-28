// Usage: node tools/benchmark/spec066/bundle-comparison.mjs BASE_DIST CANDIDATE_DIST
import fs from "node:fs";
import zlib from "node:zlib";
import {offlineAssets} from "../../../apps/web-next/scripts/offline-assets.mjs";
function measure(root,mode) {
 const m=JSON.parse(fs.readFileSync(root+"/.vite/manifest.json"));
 const entry=Object.keys(m).find(k=>m[k].isEntry);
 const files=new Set(),seen=new Set();
 function visit(k){if(seen.has(k))return;seen.add(k);const x=m[k];for(const f of [x.file,...(x.css??[]),...(x.assets??[])])files.add(f);for(const i of x.imports??[])visit(i)}
 visit(entry);
 if(mode==="candidate")visit("src/SessionEntry.tsx");
 function size(list){let raw=0,gzip=0;for(const f of list){const b=fs.readFileSync(root+"/"+f);raw+=b.length;gzip+=zlib.gzipSync(b).length}return {assets:list.length,rawBytes:raw,gzipBytes:gzip}}
 const precache=mode==="candidate"?offlineAssets(m).map(p=>p.slice(1)):fs.readdirSync(root+"/assets").map(p=>"assets/"+p);
 return {landingStaticClosure:size([...files]),offlinePrecache:size(precache)};
}
if(process.argv.length!==4)throw Error("Expected baseline and candidate dist directories");
console.log(JSON.stringify({baseline:measure(process.argv[2],"baseline"),candidate:measure(process.argv[3],"candidate"),method:"Vite manifest static dependency closure and Node gzip per file; references, not actual network transfer; excludes HTML/public/provider/API payloads.",node:process.version},null,2));
