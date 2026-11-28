// SPEC 073 T001: dependency and execution-matrix audit. No release qualification.
import {readFileSync,writeFileSync,mkdirSync} from "node:fs";
import {spawnSync} from "node:child_process";
import {createHash} from "node:crypto";
import {resolve} from "node:path";
import {pathToFileURL} from "node:url";

export function parseMatrix(input) {
  const parents = new Map(); let current;
  for (const line of input.split(/\r?\n/)) {
    const heading = /^### (T\d{3}) — (E\d{2}) \|/.exec(line);
    if (heading) {
      if (parents.has(heading[1])) throw new Error(`duplicate ${heading[1]}`);
      current = {id:heading[1],engine:heading[2],depends:null,leaves:[]};
      parents.set(current.id,current); continue;
    }
    const dep = /^Depends: (.*?)\. Owner paths: (.+)\.$/.exec(line);
    if (dep && current) {
      if (current.depends !== null) throw new Error(`duplicate dependency row ${current.id}`);
      current.depends = dep[1] === "none" ? [] : dep[1].split(/,\s*/);
      if (new Set(current.depends).size !== current.depends.length) throw new Error(`duplicate edge ${current.id}`);
      current.ownerPaths=dep[2]; continue;
    }
    const leaf = /^- \[([ xX])\] (T\d{3}\.\d+)\s+(.+)$/.exec(line);
    if (leaf) {
      if (!current || !leaf[2].startsWith(current.id+".") || current.leaves.some(x=>x.id===leaf[2])) throw new Error(`invalid leaf ${leaf[2]}`);
      current.leaves.push({id:leaf[2],checked:leaf[1]!==" ",description:leaf[3]});
    }
  }
  if (!parents.size) throw new Error("empty execution matrix");
  for (const p of parents.values()) {
    if (p.depends===null || !p.leaves.length) throw new Error(`incomplete ${p.id}`);
    for (const d of p.depends) if (!parents.has(d)) throw new Error(`unknown dependency ${d} in ${p.id}`);
  }
  const sorted=[]; const remaining=new Set(parents.keys());
  while(remaining.size) {
    const ready=[...remaining].filter(id=>parents.get(id).depends.every(d=>!remaining.has(d))).sort();
    if (!ready.length) throw new Error(`dependency cycle: ${[...remaining].join(",")}`);
    for (const id of ready) {remaining.delete(id);sorted.push(id);}
  }
  return {parents:[...parents.values()],order:sorted};
}

export function parseCargoTree(input,root) {
  const lines=input.split(/\r?\n/).map(x=>x.trim()).filter(Boolean);
  const names=lines.map(line=>{
    const match=/^([A-Za-z0-9_-]+) v[0-9]/.exec(line);
    if(!match) throw new Error(`unrecognized cargo tree row: ${line}`);
    return match[1];
  });
  if(names[0]!==root) throw new Error(`expected root ${root}, got ${names[0]??"empty"}`);
  return [...new Set(names)].sort();
}

export function forbiddenPackages(names) {
  return names.filter(n=>/^(axum(?:-|$)|sqlx(?:-|$)|postgres(?:-|$)|tokio-postgres(?:-|$)|deadpool-postgres(?:-|$)|ovg-storage$|openvietgis-api(?:-|$)|api-rust$|ovg-module-house-numbering$)/.test(n));
}

export function summarize(graph,accepted) {
  const ids=new Set(graph.parents.map(x=>x.id));
  for(const id of accepted) if(!ids.has(id)) throw new Error(`unknown accepted task ${id}`);
  for(const p of graph.parents) if(accepted.has(p.id)&&p.depends.some(d=>!accepted.has(d))) throw new Error(`accepted task has unaccepted dependency: ${p.id}`);
  return {parentCount:graph.parents.length,leafCount:graph.parents.reduce((s,p)=>s+p.leaves.length,0),checkedLeaves:graph.parents.flatMap(p=>p.leaves).filter(x=>x.checked).length,ready:graph.parents.filter(p=>!accepted.has(p.id)&&p.depends.every(d=>accepted.has(d))).map(p=>p.id),order:graph.order};
}

function run(cmd,args,cwd) {
  const r=spawnSync(cmd,args,{cwd,encoding:"utf8",maxBuffer:64*1024*1024,timeout:240000,env:{...process.env,CARGO_TERM_COLOR:"never"}});
  if(r.error || r.status!==0) throw new Error(cmd+" "+args.join(" ")+": "+(r.error?.message??r.stderr));
  return r.stdout;
}
function main() {
  const cwd=run("git",["rev-parse","--show-toplevel"],process.cwd()).trim();
  const out=resolve(process.argv[2]??"/tmp/ovg-spec073-baseline");
  mkdirSync(out,{recursive:true});
  const matrix=readFileSync(resolve(cwd,"specs/073-native-mobile-platform/execution-matrix.md"),"utf8");
  const taskText=readFileSync(resolve(cwd,"specs/073-native-mobile-platform/tasks.md"),"utf8");
  const graph=parseMatrix(matrix),stats=summarize(graph,new Set());
  const ids=[...taskText.matchAll(/^- \[[ xX]\] (T\d{3}) /gm)].map(m=>m[1]);
  if(stats.parentCount!==78||stats.leafCount!==238||new Set(ids).size!==78||ids.length!==78||graph.parents.some(p=>!ids.includes(p.id))) throw new Error("SPEC scope mismatch");
  const sha=run("git",["rev-parse","HEAD"],cwd).trim();
  const tracked=run("git",["ls-files"],cwd).split(/\r?\n/).filter(Boolean);
  const roots=["ovg-core","ovg-spatial","ovg-processing","ovg-sync","ovg-module-sdk"];
  const targets=["aarch64-linux-android","aarch64-apple-ios"];
  const report={schema:1,sha,scope:"candidate baseline only, not mobile qualification",matrixSha256:createHash("sha256").update(matrix).digest("hex"),stats,parents:graph.parents,trackedInventory:tracked.filter(p=>/^(crates\/|apps\/api-rust\/src\/|packages\/|modules\/|specs\/.*\/contracts\/)/.test(p)),results:[]};
  let failed=false;
  for(const target of targets) {
    try {
      const metadata=run("cargo",["metadata","--locked","--format-version","1","--filter-platform",target],cwd);
      writeFileSync(resolve(out,"metadata-"+target+".json"),metadata);
    } catch(e) {failed=true;report.results.push({target,kind:"metadata",status:"BLOCKED",error:String(e)});}
    for(const root of roots) {
      const args=["tree","--locked","--target",target,"--package",root,"--edges","normal,build","--prefix","none","--format","{p}"];
      try {
        const raw=run("cargo",args,cwd);
        writeFileSync(resolve(out,root+"-"+target+".txt"),raw);
        const packages=parseCargoTree(raw,root),forbidden=forbiddenPackages(packages);
        failed ||= forbidden.length>0;
        report.results.push({target,root,args,status:forbidden.length?"FAIL":"PASS",packages,forbidden});
      } catch(e) {failed=true;report.results.push({target,root,args,status:"BLOCKED",error:String(e)});}
    }
  }
  writeFileSync(resolve(out,"report.json"),JSON.stringify(report,null,2)+"\n");
  process.exitCode=failed?1:0;
}
if(process.argv[1]&&import.meta.url===pathToFileURL(resolve(process.argv[1])).href) main();
