// Run only synthetic browser cases against this checkout's built frontend.
import {preview} from "vite";
import {spawn} from "node:child_process";
import {fileURLToPath} from "node:url";
import {dirname} from "node:path";
const root=dirname(fileURLToPath(import.meta.url));
const server=await preview({root,preview:{host:"127.0.0.1",port:0,strictPort:true,open:false}});
try {
 const address=server.httpServer.address();
 if(!address||typeof address==="string"||address.address!=="127.0.0.1")throw Error("Expected loopback preview");
 const url="http://127.0.0.1:"+address.port+"/app";
 for(const script of ["spatial-mode-smoke.mjs","core-toolkit-smoke.mjs"]){
  await new Promise((resolve,reject)=>{
   const child=spawn(process.execPath,[script],{cwd:root,env:{...process.env,OVG_TEST_URL:url},stdio:"inherit"});
   const timer=setTimeout(()=>child.kill("SIGKILL"),100_000);
   child.on("error",error=>{clearTimeout(timer);reject(error)});
   child.on("exit",(code,signal)=>{clearTimeout(timer);code===0?resolve():reject(Error(script+" failed: "+(signal??code)))});
  });
 }
} finally {
 const http=server.httpServer;
 await new Promise((resolve,reject)=>{
  http.close(err=>err&&err.code!=="ERR_SERVER_NOT_RUNNING"?reject(err):resolve());
  // Browser smoke can leave keep-alive sockets behind after the child exits.
  // Drain them after close() stops accepting new connections so Node never
  // terminates an unsettled top-level await with exit code 13.
  http.closeAllConnections?.();
 });
}
