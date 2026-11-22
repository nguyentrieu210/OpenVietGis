// Synthetic only: no external or proprietary geometry.
export function gltfFixture(){
 const vertices=new Float32Array([0,0,0,25,0,0,25,0,25,0,0,25,0,25,0,25,25,0,25,25,25,0,25,25]);
 const indices=new Uint16Array([0,1,2,0,2,3,4,6,5,4,7,6,0,4,5,0,5,1,1,5,6,1,6,2,2,6,7,2,7,3,3,7,4,3,4,0]);
 const bin=new Uint8Array(vertices.byteLength+indices.byteLength);bin.set(new Uint8Array(vertices.buffer));bin.set(new Uint8Array(indices.buffer),vertices.byteLength);
 const doc={asset:{version:"2.0"},buffers:[{byteLength:bin.length}],bufferViews:[{buffer:0,byteOffset:0,byteLength:vertices.byteLength},{buffer:0,byteOffset:vertices.byteLength,byteLength:indices.byteLength}],accessors:[{bufferView:0,componentType:5126,count:8,type:"VEC3"},{bufferView:1,componentType:5123,count:36,type:"SCALAR"}],materials:[{pbrMetallicRoughness:{baseColorFactor:[1,.02,.02,1]}}],meshes:[{primitives:[{attributes:{POSITION:0},indices:1,material:0}]}],nodes:[{mesh:0,translation:[-12.5,0,-12.5]}],scenes:[{nodes:[0]}],scene:0};
 return {doc,bin}
}
export function glbFixture(){
 const {doc,bin}=gltfFixture(),json=new TextEncoder().encode(JSON.stringify(doc)),js=(json.length+3)&~3,bs=(bin.length+3)&~3;
 const buffer=new ArrayBuffer(28+js+bs),d=new DataView(buffer),a=new Uint8Array(buffer);
 d.setUint32(0,0x46546c67,true);d.setUint32(4,2,true);d.setUint32(8,buffer.byteLength,true);d.setUint32(12,js,true);d.setUint32(16,0x4e4f534a,true);
 a.fill(32,20,20+js);a.set(json,20);d.setUint32(20+js,bs,true);d.setUint32(24+js,0x004e4942,true);a.set(bin,28+js);return buffer
}
export function terrainFixture(){
 const origin=11800000,b=1200000;
 return {source_id:"dem",name:"Synthetic DEM",crs_code:3857,source_crs_code:3857,width:3,height:3,extent:[origin,b,origin+100,b+100],values:[-5,5,15,-5,5,15,-5,5,null],min_m:-5,max_m:15,valid_count:8,nodata_count:1,render_origin_m:-5,vertical_reference:"local",reference_label:"Synthetic",elevation_unit:"metre",sampled:true,survey_grade:false,declared_by_user:true}
}
