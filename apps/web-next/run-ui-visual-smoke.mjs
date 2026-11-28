// Synthetic visual regression capture; no network requests outside loopback.
import {preview} from "vite";
import {spawn} from "node:child_process";
import {mkdir,readdir} from "node:fs/promises";
import {fileURLToPath} from "node:url";
import {resolve,dirname} from "node:path";
const root=dirname(fileURLToPath(import.meta.url));
if(!process.argv[2])throw Error("Usage: node run-ui-visual-smoke.mjs NEW_SCREENSHOT_DIRECTORY");
const out=resolve(process.argv[2]);
await mkdir(out,{recursive:true});
if((await readdir(out)).length)throw Error("Screenshot directory must be empty");
const modes=[{OVG_EXPORT_ONLY:"1"},{OVG_PLUGIN071_ONLY:"1",OVG_DIALOG072_ONLY:"1"},{OVG_ACL072_ONLY:"1"},{}];
const server=await preview({root,preview:{host:"127.0.0.1",port:0,strictPort:true,open:false}});
try{
 const address=server.httpServer.address();
 if(!address||typeof address==="string"||address.address!=="127.0.0.1")throw Error("Expected loopback preview");
 for(const mode of modes){
  const env={...process.env};
  for(const key of Object.keys(env))if(/^OVG_.*ONLY$/.test(key))delete env[key];
  Object.assign(env,mode,{OVG_TEST_URL:"http://127.0.0.1:"+address.port+"/app",OVG_UI_SCREENSHOTS:out});
  await new Promise((ok,bad)=>{
   const child=spawn(process.execPath,["qgis-parity-keyboard-smoke.mjs"],{cwd:root,env,stdio:"inherit"});
   const timeout=setTimeout(()=>child.kill("SIGKILL"),100_000);
   child.on("error",err=>{clearTimeout(timeout);bad(err)});
   child.on("exit",(code,signal)=>{clearTimeout(timeout);code===0?ok():bad(Error("Visual smoke failed: "+JSON.stringify(mode)+" "+(signal??code)))});
  });
 }
 const files=(await readdir(out)).sort();
 const expected=[...["light","dark"].flatMap(t=>[360,390,640,768,1280,1920].map(w=>"export-"+t+"-"+w+".png")),...["light","dark"].flatMap(t=>[390,1280].flatMap(w=>["modeler-"+t+"-"+w+".png","acl-"+t+"-"+w+".png"])),"plugin-manager.png","plugin-manager-dark.png"].sort();
 if(JSON.stringify(files)!==JSON.stringify(expected))throw Error("Unexpected screenshot manifest: "+JSON.stringify(files));
 console.log("PASS visual capture manifest: "+files.length+" images");
}finally{server.httpServer.close();server.httpServer.closeAllConnections?.();server.httpServer.unref()}
