import {describe,it,expect} from "vitest";
import {readNetcdf4Variables,type HdfVariable} from "./netcdf4";
import {parse2dm} from "./local-mesh";
import * as h from "h5wasm/node";
const mesh=parse2dm("MESH2D\nND 3 106 10 0\nND 1 106.1 10 0\nND 2 106 10.1 0\nE3T 1 3 1 2 1",4326);
describe("NetCDF4 HDF5 actual compressed arrays",()=>{
 it("reads node/time scales, compressed scalar slices and vector components",async()=>{
  await h.ready;
  const path="/tmp/ovg-spec065-netcdf4-"+Date.now()+".h5";
  const fs=h.FS;if(!fs)throw Error("Missing HDF5 filesystem");
  let file=new h.File(path,"w");
  try{
   const node=file.create_dataset({name:"node_id",data:Int32Array.from([1,2,3])});node.make_scale("node_id");
   const time=file.create_dataset({name:"time",data:Float64Array.from([0,1])});time.make_scale("time");time.create_attribute("units","hours since 2026-01-01 00:00:00 UTC");
   const field=(name:string,values:number[])=>{const ds=file.create_dataset({name,data:Float32Array.from(values),shape:[2,3],chunks:[1,3],compression:4});ds.attach_scale(0,"/time");ds.attach_scale(1,"/node_id");ds.create_attribute("units","m");ds.create_attribute("_FillValue",Float32Array.of(-9999));return ds};
   field("depth",[1,2,3,4,-9999,6]);field("other",[0,0,4,0,0,8]);
   file.close();file=new h.File(path,"r");
   const get=(name:string)=>{const v=file.get(name);if(!(v instanceof h.Dataset))throw Error("Missing fixture");return v as unknown as HdfVariable};
   expect(readNetcdf4Variables(get,mesh,"depth").frames).toEqual([[3,1,2],[6,4,null]]);
   expect(readNetcdf4Variables(get,mesh,"depth","other").frames).toEqual([[5,1,2],[10,4,null]]);
  }finally{try{file.close()}finally{fs.unlink(path)}}
 },30000);
});
