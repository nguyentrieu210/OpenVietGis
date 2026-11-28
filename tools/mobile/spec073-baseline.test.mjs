import test from "node:test";
import assert from "node:assert/strict";
import {readFileSync} from "node:fs";
import {parseMatrix,parseCargoTree,forbiddenPackages,summarize} from "./spec073-baseline.mjs";
const actual=readFileSync(new URL("../../specs/073-native-mobile-platform/execution-matrix.md",import.meta.url),"utf8");
test("actual matrix has all parents and leaves, valid DAG, only T001 initially ready",()=>{
 const g=parseMatrix(actual),s=summarize(g,new Set());
 assert.equal(s.parentCount,78); assert.equal(s.leafCount,238); assert.deepEqual(s.ready,["T001"]);
 assert.deepEqual(summarize(g,new Set(["T001"])).ready,["T002","T005","T006","T007"]);
});
test("cycles, unknown dependencies and acceptance out of order fail closed",()=>{
 assert.throws(()=>parseMatrix(actual.replace("Depends: none.","Depends: T078.")),/cycle/);
 assert.throws(()=>parseMatrix(actual.replace("Depends: none.","Depends: T999.")),/unknown dependency/);
 assert.throws(()=>summarize(parseMatrix(actual),new Set(["T009"])),/unaccepted dependency/);
});
test("Cargo graph rejects server package, empty and unexpected output",()=>{
 const p=parseCargoTree("ovg-core v0.2.0 (/repo)\nserde v1.0.1\nsqlx-postgres v0.8.1 (*)\n","ovg-core");
 assert.deepEqual(forbiddenPackages(p),["sqlx-postgres"]);
 assert.deepEqual(forbiddenPackages(["serde","ovg-spatial","axum-core","tokio-postgres","ovg-storage"]),["axum-core","tokio-postgres","ovg-storage"]);
 assert.throws(()=>parseCargoTree("","ovg-core"),/expected root/);
 assert.throws(()=>parseCargoTree("ovg-core v0.2.0\nmalformed","ovg-core"),/unrecognized/);
});
