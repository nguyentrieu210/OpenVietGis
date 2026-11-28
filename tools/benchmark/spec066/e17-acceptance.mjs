import {spawnSync} from "node:child_process";
import {execFileSync} from "node:child_process";
import {resolve} from "node:path";

const [rootArg,label]=process.argv.slice(2);
if(!rootArg||!label)throw Error("usage: e17-acceptance.mjs ROOT LABEL");
const root=resolve(rootArg);
const script=resolve(root,"tests/e17/benchmark.mjs");
const result=spawnSync(process.execPath,[script,root],{cwd:root,encoding:"utf8",timeout:300000});
if(result.error)throw result.error;
if(result.status!==0)throw Error(result.stderr||`E17 benchmark exited with ${result.status}`);
const sourceSha=execFileSync("git",["-C",root,"rev-parse","HEAD"],{encoding:"utf8"}).trim();
const report=JSON.parse(result.stdout);
console.log(JSON.stringify({label,source_sha:sourceSha,report},null,2));
