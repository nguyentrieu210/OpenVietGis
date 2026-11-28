import {test} from "node:test";
import assert from "node:assert/strict";
import {offlineAssets} from "./scripts/offline-assets.mjs";
const fixture = {
 "index.html":{isEntry:true,file:"assets/main.js",css:["assets/base.css"],imports:["_react.js"],dynamicImports:["src/offline-entry.tsx","src/LiveApp.tsx"]},
 "_react.js":{file:"assets/react.js"},
 "src/offline-entry.tsx":{file:"assets/offline.js",imports:["_react.js","_shared.js"],css:["assets/offline.css"]},
 "_shared.js":{file:"assets/shared.js",assets:["assets/icon.svg"]},
 "src/LiveApp.tsx":{file:"assets/gis.js",assets:["assets/heavy.wasm"]}
};
test("precaches entry + offline static closure, not unrelated GIS/CAD workbenches",()=>{
 assert.deepEqual(offlineAssets(fixture),["/assets/base.css","/assets/icon.svg","/assets/main.js","/assets/offline.css","/assets/offline.js","/assets/react.js","/assets/shared.js"]);
});
test("fails closed if offline dependency is missing or unsafe",()=>{
 assert.throws(()=>offlineAssets({...fixture,"_shared.js":undefined}),/Missing offline dependency/);
 assert.throws(()=>offlineAssets({...fixture,"_shared.js":{file:"../private"}}),/Invalid offline asset/);
 assert.throws(()=>offlineAssets({"index.html":fixture["index.html"]}),/offline entry/);
});
