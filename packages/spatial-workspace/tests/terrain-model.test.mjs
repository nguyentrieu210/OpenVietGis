import assert from "node:assert/strict";
import {test} from "node:test";
import {validateTerrain,sampleTerrain,terrainTriangles,geographic3857} from "../src/terrain.ts";
import {parseStaticModel} from "../src/gltf.ts";
import {glbFixture,gltfFixture,terrainFixture} from "./terrain-model-fixture.mjs";
test("terrain validation, negative metres preserved, same triangle interpolation, holes and outside",()=>{
 const g=validateTerrain(terrainFixture(),"dem"),[l,b,r,t]=g.extent;
 assert.equal(terrainTriangles(g).length,18);
 assert.equal(sampleTerrain(g,...geographic3857(l,t)),-5);
 assert.ok(Math.abs(sampleTerrain(g,...geographic3857(l+25,t-25)))<1e-6);
 assert.equal(sampleTerrain(g,...geographic3857(r-10,b+10)),null);
 assert.equal(sampleTerrain(g,...geographic3857(l-1,t)),null);
 assert.throws(()=>validateTerrain({...g,min_m:0},"dem"));
 assert.throws(()=>validateTerrain(g,"other"));
});
test("GLB and self-contained GLTF same transformed indices, material and geometry",()=>{
 const model=parseStaticModel(glbFixture(),"glb");assert.equal(model.triangles,12);assert.equal(model.nodes,1);assert.equal(model.positions.length,108);assert.deepEqual([...model.positions.slice(0,3)],[-12.5,0,-12.5]);
 const {doc,bin}=gltfFixture();doc.buffers[0].uri="data:application/octet-stream;base64,"+Buffer.from(bin).toString("base64");
 const gltf=parseStaticModel(new TextEncoder().encode(JSON.stringify(doc)).buffer,"gltf");
 assert.deepEqual(gltf.positions,model.positions);assert.deepEqual(gltf.colors,model.colors);
 doc.buffers[0].uri="https://example.invalid/model.bin";assert.throws(()=>parseStaticModel(new TextEncoder().encode(JSON.stringify(doc)).buffer,"gltf"),/URI ngoài/);
});
test("GLB corrupt header and unsupported data rejected",()=>{
 const wrong=glbFixture();new DataView(wrong).setUint32(4,1,true);assert.throws(()=>parseStaticModel(wrong,"glb"));
 const {doc,bin}=gltfFixture();doc.buffers[0].uri="data:application/octet-stream;base64,"+Buffer.from(bin).toString("base64");doc.nodes[0].children=[0];assert.throws(()=>parseStaticModel(new TextEncoder().encode(JSON.stringify(doc)).buffer,"gltf"),/vòng lặp/);
});
