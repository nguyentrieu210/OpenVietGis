import {defineConfig} from "vite";
import tailwindcss from "@tailwindcss/vite";
import {cpSync,mkdirSync,existsSync,readFileSync} from "node:fs";
import {fileURLToPath} from "node:url";
import {join} from "node:path";
const pkg=fileURLToPath(new URL("../../packages/spatial-workspace/node_modules/cesium/",import.meta.url));
const source=join(pkg,"Build/Cesium");
const version=JSON.parse(readFileSync(join(pkg,"package.json"),"utf8")).version;
if(version!=="1.147.0")throw Error("Update the reviewed Cesium version and its runtime path together.");
const target=fileURLToPath(new URL("./public/cesium/"+version+"/",import.meta.url));
// Use the pinned, prebuilt ESM distribution. Avoid re-bundling the 3D engine.
if(!existsSync(join(source,"index.js")))throw Error("Install the locked Cesium package first.");
mkdirSync(target,{recursive:true});
for(const name of ["index.js","Assets","Workers","Widgets","ThirdParty"])cpSync(join(source,name),join(target,name),{recursive:true});
for(const name of ["LICENSE.md","CONTRIBUTORS.md"])cpSync(join(pkg,name),join(target,name));

const hdfLicense=fileURLToPath(new URL("./node_modules/h5wasm/LICENSE.txt",import.meta.url));
mkdirSync(fileURLToPath(new URL("./public/licenses/",import.meta.url)),{recursive:true});
cpSync(hdfLicense,fileURLToPath(new URL("./public/licenses/h5wasm.txt",import.meta.url)));
const retired=fileURLToPath(new URL("../web/",import.meta.url));
const port=process.env.OPENVIETGIS_API_PROXY_PORT??"8000";
 if(!/^\d{1,5}$/.test(port)||Number(port)<1||Number(port)>65535)throw Error("Invalid loopback API port");
 const proxy={"/api":{target:"http://127.0.0.1:"+Number(port),changeOrigin:false}};
export default defineConfig({worker:{format:"es"},build:{manifest:true},server:{proxy},preview:{proxy},
 plugins:[tailwindcss(),{name:"canonical-frontend-boundary",load(id){
  if(id.split("?")[0].startsWith(retired))throw new Error("Retired frontend dependency: "+id);
 }}]
 });
