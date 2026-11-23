import {test} from "node:test";import assert from "node:assert/strict";
import {footprint3d} from "../src/footprint3d.ts";
const geometry={type:"Polygon",coordinates:[[[106,11],[106.001,11],[106.001,11.001],[106,11]]]};
const f={id:"a",geometry,baseMetres:1,heightMetres:8};
test("explicit base and height form actual top, identity retained",()=>{const result=footprint3d([f],"EPSG:4326","relative-flat-reference");assert.deepEqual(result.features[0].properties,{featureId:"a",base:1,top:9});assert.notEqual(result.features[0].geometry,geometry)});
test("missing or invalid heights/duplicate ids rejected",()=>{for(const entry of [{...f,heightMetres:NaN},{...f,heightMetres:0},{...f,baseMetres:-1}])assert.throws(()=>footprint3d([entry],"EPSG:4326","relative-flat-reference"));assert.throws(()=>footprint3d([f,f],"EPSG:4326","relative-flat-reference"))});
test("non-4326 or unspecified vertical reference rejected",()=>{assert.throws(()=>footprint3d([f],"EPSG:3857","relative-flat-reference"));assert.throws(()=>footprint3d([f],"EPSG:4326","ellipsoid"))});
test("unclosed polygon and geographic out of range rejected",()=>{for(const coords of [[[[106,11],[107,11],[107,12],[106,12]]],[[[181,11],[107,11],[107,12],[181,11]]]])assert.throws(()=>footprint3d([{...f,geometry:{type:"Polygon",coordinates:coords}}],"EPSG:4326","relative-flat-reference"))});
