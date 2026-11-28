import assert from "node:assert/strict";
import test from "node:test";
import { buildLayerTree, flatten, layerIds, parentPath, renameGroup } from "./src/layer-tree.ts";

test("groups nest by path and sit at their first layer's position", () => {
  const groups = { b: "Hiện trạng/Giao thông", c: "Hiện trạng", d: "Hiện trạng/Giao thông" };
  const tree = buildLayerTree(["a", "b", "c", "d", "e"], id => groups[id]);
  assert.deepEqual(tree.map(n => (n.kind === "layer" ? n.id : n.path)), ["a", "Hiện trạng", "e"]);
  const ht = tree[1];
  assert.equal(ht.kind, "group");
  assert.deepEqual(ht.children.map(n => (n.kind === "layer" ? n.id : n.path)), ["Hiện trạng/Giao thông", "c"]);
  assert.deepEqual(layerIds(ht), ["b", "d", "c"]);
  assert.equal(flatten(tree).length, 7);
});

test("rename moves sub-groups; ungroup goes one level up", () => {
  const changed = renameGroup({ a: "A", b: "A/B", c: "AB", d: "X" }, "A", "Z");
  assert.deepEqual(changed, { a: "Z", b: "Z/B" });
  assert.equal(parentPath("A/B"), "A");
  assert.equal(parentPath("A"), null);
});

import {visibleLayerIds,layerRange,groupVisibilityChanges} from "./src/layer-tree.ts";
test("keyboard and shift ranges exclude collapsed and filtered layers",()=>{
 const tree=buildLayerTree(["a","b","c","d"],id=>id==="b"||id==="c"?"G":undefined);
 assert.deepEqual(visibleLayerIds(tree,()=>false,()=>true),["a","d"]);
 assert.deepEqual(visibleLayerIds(tree,()=>true,id=>id!=="b"),["a","c","d"]);
 assert.deepEqual(layerRange(["a","c","d"],"a","d"),["a","c","d"]);
 assert.deepEqual(layerRange(["a","c","d"],"b","d"),["d"]);
});
test("mixed group checkbox shows only hidden layers, then hides all",()=>{
 assert.deepEqual(groupVisibilityChanges(["a","b"],new Set(["b"])),["b"]);
 assert.deepEqual(groupVisibilityChanges(["a","b"],new Set()),["a","b"]);
});
