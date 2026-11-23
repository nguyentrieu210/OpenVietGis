/** Dependency-free, local-only static glTF 2.0 subset. No URL I/O, scripts or remote resources. */
export const MODEL_MAX_BYTES=10*1024*1024;
const MAX_TRIANGLES=100000, MAX_NODES=256;
export type StaticModel=Readonly<{positions:Float32Array;colors:Float32Array;triangles:number;nodes:number}>;
const fail=(reason:string):never=>{throw Error("GLB/GLTF: "+reason)};
const int=(v:unknown,min:number,max:number):number=>typeof v==="number"&&Number.isInteger(v)&&v>=min&&v<=max?v:fail("số nguyên ngoài giới hạn");
const vector=(v:unknown,length:number,max=10000):number[]=>Array.isArray(v)&&v.length===length&&v.every(x=>typeof x==="number"&&Number.isFinite(x)&&Math.abs(x)<=max)?v:fail("vector hoặc ma trận không hợp lệ");
const I=[1,0,0,0,0,1,0,0,0,0,1,0,0,0,0,1];
const multiply=(a:number[],b:number[])=>Array.from({length:16},(_,q)=>{const c=Math.floor(q/4),r=q%4;return a[r]!*b[c*4]!+a[4+r]!*b[c*4+1]!+a[8+r]!*b[c*4+2]!+a[12+r]!*b[c*4+3]!});
function matrix(n:any):number[]{
 if(n.matrix!==undefined){if(n.translation||n.rotation||n.scale)fail("matrix xung đột TRS");const m=vector(n.matrix,16);if(m[3]!==0||m[7]!==0||m[11]!==0||m[15]!==1)fail("matrix không affine");return m}
 const [x,y,z,w]=vector(n.rotation??[0,0,0,1],4,1) as [number,number,number,number],s=vector(n.scale??[1,1,1],3,1000),t=vector(n.translation??[0,0,0],3);
 if(Math.abs(x*x+y*y+z*z+w*w-1)>1e-3||s.some(v=>v===0))fail("quaternion hoặc scale không hợp lệ");
 const r=[1-2*(y*y+z*z),2*(x*y+w*z),2*(x*z-w*y),0,2*(x*y-w*z),1-2*(x*x+z*z),2*(y*z+w*x),0,2*(x*z+w*y),2*(y*z-w*x),1-2*(x*x+y*y),0,t[0]!,t[1]!,t[2]!,1];
 for(let j=0;j<3;j++)for(let k=0;k<3;k++)r[j*4+k]=r[j*4+k]!*s[j]!;return r;
}
function decodeFile(bytes:ArrayBuffer,extension:"glb"|"gltf"):{doc:any;bin:Uint8Array}{
 if(bytes.byteLength<4||bytes.byteLength>MODEL_MAX_BYTES)fail("tệp không vượt quá 10 MiB");
 const decoder=new TextDecoder("utf-8",{fatal:true});
 if(extension==="glb"){
  const v=new DataView(bytes);if(bytes.byteLength<20||v.getUint32(0,true)!==0x46546c67||v.getUint32(4,true)!==2||v.getUint32(8,true)!==bytes.byteLength)fail("GLB v2 không hợp lệ");
  const jsonSize=v.getUint32(12,true),end=20+jsonSize;if(jsonSize%4!==0||v.getUint32(16,true)!==0x4e4f534a||end+8>bytes.byteLength)fail("thiếu JSON hoặc BIN");
  const doc=JSON.parse(decoder.decode(new Uint8Array(bytes,20,jsonSize))),size=v.getUint32(end,true);
  if(size%4!==0||v.getUint32(end+4,true)!==0x004e4942||end+8+size!==bytes.byteLength)fail("chỉ một BIN chunk được hỗ trợ");
  return {doc,bin:new Uint8Array(bytes,end+8,size)};
 }
 const doc=JSON.parse(decoder.decode(bytes)),uri=doc?.buffers?.[0]?.uri;
 if(typeof uri!=="string"||!/^data:application\/(?:octet-stream|gltf-buffer);base64,[A-Za-z0-9+/]+={0,2}$/.test(uri))fail("GLTF phải tự chứa buffer base64, không tải URI ngoài");
 const binary=atob(uri.slice(uri.indexOf(",")+1));return {doc,bin:Uint8Array.from(binary,c=>c.charCodeAt(0))};
}
export function parseStaticModel(bytes:ArrayBuffer,extension:"glb"|"gltf"):StaticModel{
 const {doc,bin}=decodeFile(bytes,extension);
 if(!doc||doc.asset?.version!=="2.0"||doc.extensionsUsed?.length||doc.extensionsRequired?.length||doc.animations?.length||doc.skins?.length||doc.images?.length||doc.textures?.length||doc.cameras?.length)fail("chỉ mesh tĩnh glTF 2.0, không extension, ảnh, texture, animation, skin hoặc camera");
 if(!Array.isArray(doc.buffers)||doc.buffers.length!==1||!Array.isArray(doc.bufferViews)||!Array.isArray(doc.accessors)||!Array.isArray(doc.meshes)||!Array.isArray(doc.nodes)||!Array.isArray(doc.scenes)||doc.nodes.length>MAX_NODES||doc.meshes.length>MAX_NODES||doc.accessors.length>1024||doc.bufferViews.length>1024)fail("cấu trúc hoặc giới hạn cảnh");
 if(extension==="glb"&&doc.buffers[0].uri!==undefined)fail("GLB không được tham chiếu URI");const used=int(doc.buffers[0].byteLength,1,bin.byteLength);if(bin.byteLength-used>3)fail("kích thước BIN không khớp");
 const view=new DataView(bin.buffer,bin.byteOffset,used);
 const accessor=(index:unknown,position:boolean)=>{
  const a=doc.accessors[int(index,0,doc.accessors.length-1)];
  if(!a||a.sparse||a.normalized||a.type!==(position?"VEC3":"SCALAR")||(position?a.componentType!==5126:![5121,5123,5125].includes(a.componentType)))fail("accessor POSITION/indices không được hỗ trợ");
  const b=doc.bufferViews[int(a.bufferView,0,doc.bufferViews.length-1)];if(!b||b.buffer!==0||b.extensions)fail("bufferView ngoài hoặc nén");
  const count=int(a.count,1,MAX_TRIANGLES*3),component=position?4:a.componentType===5121?1:a.componentType===5123?2:4,element=component*(position?3:1),stride=int(b.byteStride??element,element,252);
  const bo=int(b.byteOffset??0,0,used),len=int(b.byteLength,1,used),off=int(a.byteOffset??0,0,len);
  if(stride%component||bo%component||off%component||bo+len>used||off+(count-1)*stride+element>len)fail("accessor vượt buffer");
  return {count,read:(i:number,j=0)=>{const p=bo+off+i*stride+j*component;return position?view.getFloat32(p,true):component===1?view.getUint8(p):component===2?view.getUint16(p,true):view.getUint32(p,true)}};
 };
 const pos:number[]=[],colors:number[]=[];let triangles=0;const seen=new Set<number>();
 const roots=doc.scenes[int(doc.scene??0,0,doc.scenes.length-1)]?.nodes;if(!Array.isArray(roots)||!roots.length)fail("cảnh rỗng");
 const walk=(id:unknown,parent:number[],depth:number)=>{
  const index=int(id,0,doc.nodes.length-1);if(depth>32||seen.has(index))fail("node trùng, vòng lặp hoặc quá sâu");seen.add(index);const n=doc.nodes[index];if(!n||n.skin!==undefined||n.weights||n.extensions)fail("node không được hỗ trợ");const m=multiply(parent,matrix(n));if(m.some(v=>!Number.isFinite(v)||Math.abs(v)>1e6))fail("biến đổi quá lớn");
  if(n.mesh!==undefined){const mesh=doc.meshes[int(n.mesh,0,doc.meshes.length-1)];if(!Array.isArray(mesh.primitives)||mesh.primitives.length>128||mesh.weights)fail("mesh không được hỗ trợ");for(const p of mesh.primitives){
   if((p.mode??4)!==4||!p.attributes||p.targets||p.extensions)fail("chỉ triangle primitives tĩnh");
   const xyz=accessor(p.attributes.POSITION,true),indices=p.indices===undefined?null:accessor(p.indices,false),count=indices?.count??xyz.count;
   if(count%3||triangles+count/3>MAX_TRIANGLES)fail("tối đa 100.000 tam giác");triangles+=count/3;
   const material=p.material===undefined?null:doc.materials?.[int(p.material,0,(doc.materials?.length??0)-1)];
   if(p.material!==undefined&&!material||material?.alphaMode&&material.alphaMode!=="OPAQUE")fail("chỉ vật liệu opaque");
   const c=vector(material?.pbrMetallicRoughness?.baseColorFactor??[.74,.76,.8,1],4,1);if(c.some(v=>v<0)||c[3]!==1)fail("màu vật liệu không hợp lệ");
   const vertices:number[][]=[];
   for(let i=0;i<count;i++){const a=int(indices?indices.read(i):i,0,xyz.count-1),x=xyz.read(a,0),y=xyz.read(a,1),z=xyz.read(a,2);const q=[m[0]!*x+m[4]!*y+m[8]!*z+m[12]!,m[1]!*x+m[5]!*y+m[9]!*z+m[13]!,m[2]!*x+m[6]!*y+m[10]!*z+m[14]!];if(q.some(v=>!Number.isFinite(v)||Math.abs(v)>10000))fail("đỉnh không hữu hạn hoặc quá lớn");vertices.push(q)}
   for(let i=0;i<vertices.length;i+=3){const a=vertices[i]!,b=vertices[i+1]!,d=vertices[i+2]!,u=b.map((v,j)=>v-a[j]!),v=d.map((v,j)=>v-a[j]!);const nx=u[1]!*v[2]!-u[2]!*v[1]!,ny=u[2]!*v[0]!-u[0]!*v[2]!,nz=u[0]!*v[1]!-u[1]!*v[0]!,norm=Math.hypot(nx,ny,nz);const shade=norm>0?.55+.45*Math.abs((nx*.3+ny*.85+nz*.43)/norm):.55;for(const q of [a,b,d]){pos.push(...q);colors.push(c[0]!*shade,c[1]!*shade,c[2]!*shade)}}
  }}
  if(n.children!==undefined){if(!Array.isArray(n.children)||n.children.length>MAX_NODES)fail("node children không hợp lệ");for(const child of n.children)walk(child,m,depth+1)}
 };
 for(const root of roots)walk(root,I,0);if(!triangles)fail("không có tam giác");
 return {positions:new Float32Array(pos),colors:new Float32Array(colors),triangles,nodes:seen.size};
}
