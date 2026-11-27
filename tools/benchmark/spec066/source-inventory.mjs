// Read-only, commit-pinned inventory. No credentials or live data.
import {execFileSync} from "node:child_process";
const git=(args)=>execFileSync("git",args,{encoding:"utf8",maxBuffer:32*1024*1024});
const ref=process.argv[2]??"HEAD";
const sha=git(["rev-parse","--verify",ref+"^{commit}"]).trim();
const rows=git(["ls-tree","-rlz",sha]).split("\0").filter(Boolean).map(line=>{
 const [metadata,...pathParts]=line.split("\t");
 const [mode,type,oid,size]=metadata.trim().split(/\s+/);
 return {path:pathParts.join("\t"),type,mode,oid,bytes:Number(size)||0};
});
const buckets={};
for(const r of rows){const pieces=r.path.split("/"),key=pieces.length>1?pieces.slice(0,["apps","crates","packages","modules","integrations"].includes(pieces[0])?2:1).join("/"):"root";const b=buckets[key]??={files:0,bytes:0};b.files++;b.bytes+=r.bytes}
const workflows=rows.filter(r=>r.path.startsWith(".github/workflows/")).map(r=>r.path);
const manifest=JSON.parse(git(["show",sha+":package.json"]));
const scripts=Object.entries(manifest.scripts??{}).filter(([,v])=>/apps\/(?:api|web)(?:[\s/'"]|$)|\buv\b/.test(v)).map(([name,command])=>({name,command}));
console.log(JSON.stringify({sha,files:rows.length,bytes:rows.reduce((a,r)=>a+r.bytes,0),buckets,workflows,legacyScriptReferences:scripts,notes:"Tracked inventory only: not proof of dead code or deployed runtime role"},null,2));
