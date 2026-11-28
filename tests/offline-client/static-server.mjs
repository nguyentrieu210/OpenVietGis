// Isolated static + synthetic API fixture server. Never proxies to a live API.
import { createServer } from "node:http";
import { createHash } from "node:crypto";
import { readFile, readdir } from "node:fs/promises";
import { fileURLToPath } from "node:url";
import { join, extname } from "node:path";
const root = fileURLToPath(new URL("../../apps/web-next/dist/", import.meta.url));
const files = new Map([["/index.html","index.html"],["/offline-shell-sw.js","offline-shell-sw.js"]]);
for (const name of await readdir(join(root,"assets"))) files.set("/assets/"+name, join("assets",name));
const mime = {".html":"text/html; charset=utf-8",".js":"text/javascript",".mjs":"text/javascript",".css":"text/css",".png":"image/png",".svg":"image/svg+xml"};
const expires = new Date(Date.now()+3600_000).toISOString();
const gpkg = Buffer.from("SQLite format 3\0synthetic checksum fixture; native GPKG tested by Python");
const snapshot = {
  version:1,package_id:"p1",actor_id:"actor1",organization_id:"org1",project_id:"prj1",
  layer_id:"layer1",layer_name:"Poles",source_id:"src1",content_revision:"revision1",schema_revision:"schema1",
  crs_code:4326,geometry_family:"Point",aoi_bbox:{min_x:105,min_y:20,max_x:106,max_y:21},
  created_at:new Date().toISOString(),expires_at:expires,
  fields:[{field_id:"status",name:"Status",type:"text",nullable:false}],
  features:[{feature_id:"pole1",properties:{status:"baseline"},geometry:{type:"Point",coordinates:[105.5,20.5]}}]
};
const projection = Buffer.from(JSON.stringify(snapshot));
const digest = buf=>createHash("sha256").update(buf).digest("hex");
const metadata = {id:"p1",name:"Synthetic bundle",profile:"standard_pwa",crs_code:4326,
  feature_count:1,sha256:digest(gpkg),byte_size:gpkg.length,revoked_at:null,expires_at:expires,
  layer_pins:[{layer_id:"layer1",source_id:"src1",content_revision:"revision1",schema_revision:"schema1",snapshot_sha256:digest(projection)}]};
const submissions = [];
const project = "/api/v1/organizations/org1/projects/prj1";
function send(res,status,body,type="application/json") {
  res.writeHead(status,{"Content-Type":type,"Cache-Control":"no-store"});
  res.end(type==="application/json" && !(body instanceof Buffer) ? JSON.stringify(body) : body);
}
const server = createServer(async (req,res)=>{
  const path = new URL(req.url ?? "/", "http://127.0.0.1").pathname;
  if (req.method==="GET") {
    if(path==="/api/v1/me") return send(res,200,{id:"actor1",display_name:"Fixture",status:"active",email_snapshot:null});
    if(path==="/api/v1/auth/providers") return send(res,200,[]);
    if(path===project+"/layers") return send(res,200,{items:[]});
    if(path===project+"/field/packages") return send(res,200,{items:[metadata]});
    if(path===project+"/field/packages/p1/download") return send(res,200,gpkg,"application/geopackage+sqlite3");
    if(path===project+"/field/packages/p1/snapshot") return send(res,200,projection);
    if(path==="/api/v1/auth/csrf") return send(res,200,{token:"synthetic-token"});
    if(path==="/__fixture/submits") return send(res,200,submissions);
  }
  if (req.method==="POST" && path===project+"/field/packages/p1/sync") {
    const chunks = [];
    for await (const chunk of req) chunks.push(chunk);
    let payload;
    try {
      payload=JSON.parse(Buffer.concat(chunks).toString("utf8"));
      if(typeof payload.operation_id!=="string"||!Array.isArray(payload.operations))throw new Error("invalid sync payload");
      submissions.push(payload);
    } catch { return send(res,400,{code:"BAD_FIXTURE"}); }
    if(submissions.length===1) return send(res,503,{code:"ACK_LOST"});
    return send(res,201,{
      id:"receipt1",package_id:"p1",operation_id:payload.operation_id,submitted_ops_count:payload.operations.length,
      status:"applied",applied_changeset_ids:["cs1"],conflicts:[],
      operation_receipts:payload.operations.map(operation=>({
        client_operation_id:operation.client_operation_id,local_feature_id:operation.feature_id,status:"applied",
        changeset_id:"cs1",core_feature_id:operation.op_type==="update"?operation.feature_id:"core-"+operation.feature_id
      }))
    });
  }
  const name = files.get(path) ?? ((path==="/" || path==="/app" || /^\/offline-field\/[^/]+\/[^/]+\/?$/.test(path)) ? "index.html" : null);
  if (!name || req.method !== "GET") return send(res,path.startsWith("/api/") ? 503 : 404,{fixture:"No API forwarding"});
  try { return send(res,200,await readFile(join(root,name)),mime[extname(name)]??"application/octet-stream"); }
  catch { return send(res,404,{fixture:"static missing"}); }
});
server.listen(5184,"127.0.0.1");
