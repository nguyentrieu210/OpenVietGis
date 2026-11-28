import {it,expect} from "vitest";
import {readFileSync} from "node:fs";
import type {Geometry} from "geojson";
import {createWasmMetrics} from "./geometry-wasm";
import {derive} from "./geom-measure";
it("Rust WASM preserves the existing spherical measurement contract",async()=>{
 const kernel=await createWasmMetrics(readFileSync(new URL("./wasm/geometry.wasm",import.meta.url)));
 const ring=[[106,10],[106.001,10],[106.001,10.001],[106,10.001],[106,10]];
 const inputs:Geometry[]=[{type:"Point",coordinates:[106,10]},{type:"LineString",coordinates:ring},{type:"Polygon",coordinates:[ring]},{type:"MultiPolygon",coordinates:[[ring],[ring]]},{type:"MultiLineString",coordinates:[ring,ring]}];
 for(const g of inputs){const a=derive(g),b=derive(g,kernel);expect(b.vertices).toBe(a.vertices);for(const key of ["area","perimeter","length"] as const){if(a[key]!==undefined)expect(Math.abs(b[key]!-a[key]!)).toBeLessThan(Math.max(1e-6,Math.abs(a[key]!)*1e-9))}}
});
