import {MercatorCoordinate,type CustomLayerInterface} from "maplibre-gl";
import {geographic3857,terrainTriangles,type TerrainGrid} from "./terrain";
import type {StaticModel} from "./gltf";
type Vec3=[number,number,number];
export type SceneMesh={origin:Vec3;vertices:Float32Array;colors:Float32Array};
export type PlacedModel={model:StaticModel;longitude:number;latitude:number;altitude:number;rotationDeg:number;scale:number};
export function terrainMesh(grid:TerrainGrid):SceneMesh{
 const [l,b,r,t]=grid.extent,center=geographic3857((l+r)/2,(b+t)/2),o=MercatorCoordinate.fromLngLat(center,0),origin:Vec3=[o.x,o.y,o.z],vertices:number[]=[],colors:number[]=[];
 const points=grid.values.map((h,i)=>{if(h===null)return null;const col=i%grid.width,row=Math.floor(i/grid.width),xy=geographic3857(l+(r-l)*col/(grid.width-1),t-(t-b)*row/(grid.height-1));return MercatorCoordinate.fromLngLat(xy,h-grid.render_origin_m)});
 const range=Math.max(1,grid.max_m-grid.min_m);
 for(const i of terrainTriangles(grid)){const q=points[i]!,h=grid.values[i]!,s=(h-grid.min_m)/range;vertices.push(q.x-origin[0],q.y-origin[1],q.z-origin[2]);colors.push(.17+.55*s,.39+.36*s,.22+.39*s)}
 return {origin,vertices:new Float32Array(vertices),colors:new Float32Array(colors)};
}
export function modelMesh(p:PlacedModel):SceneMesh{
 if(![p.longitude,p.latitude,p.altitude,p.rotationDeg,p.scale].every(Number.isFinite)||Math.abs(p.longitude)>180||Math.abs(p.latitude)>85||p.altitude< -12000||p.altitude>22000||Math.abs(p.rotationDeg)>360||p.scale<.01||p.scale>100)throw Error("Vị trí mô hình không hợp lệ");
 const o=MercatorCoordinate.fromLngLat([p.longitude,p.latitude],p.altitude),k=o.meterInMercatorCoordinateUnits(),a=p.rotationDeg*Math.PI/180,c=Math.cos(a),s=Math.sin(a),v=p.model.positions,out=new Float32Array(v.length);
 for(let i=0;i<v.length;i+=3){const x=v[i]!*p.scale,y=v[i+1]!*p.scale,z=v[i+2]!*p.scale;if(Math.max(Math.abs(x),Math.abs(y),Math.abs(z))>10000)throw Error("Kích thước mô hình vượt 10 km");out[i]=(x*c-z*s)*k;out[i+1]=(x*s+z*c)*k;out[i+2]=y*k}
 return {origin:[o.x,o.y,o.z],vertices:out,colors:p.model.colors};
}
/** Mercator-only mesh with translated local coordinates to preserve precision; no secondary GL context. */
export function customMeshLayer(id:string,mesh:SceneMesh):CustomLayerInterface{
 let program:WebGLProgram|null=null,vao:WebGLVertexArrayObject|null=null,position:WebGLBuffer|null=null,color:WebGLBuffer|null=null,uniform:WebGLUniformLocation|null=null;
 const release=(gl:WebGL2RenderingContext)=>{if(vao)gl.deleteVertexArray(vao);if(position)gl.deleteBuffer(position);if(color)gl.deleteBuffer(color);if(program)gl.deleteProgram(program);vao=null;position=null;color=null;program=null};
 const shader=(gl:WebGL2RenderingContext,type:number,source:string)=>{const obj=gl.createShader(type);if(!obj)throw Error("WebGL shader allocation failed");gl.shaderSource(obj,source);gl.compileShader(obj);if(!gl.getShaderParameter(obj,gl.COMPILE_STATUS)){gl.deleteShader(obj);throw Error("WebGL mesh shader failed")}return obj};
 return {id,type:"custom",renderingMode:"3d",
  onAdd(_map,gl){try{
   const vs=shader(gl,gl.VERTEX_SHADER,"#version 300 es\nlayout(location=0) in vec3 a_pos;layout(location=1) in vec3 a_color;uniform mat4 u_matrix;out vec3 v_color;void main(){gl_Position=u_matrix*vec4(a_pos,1.0);v_color=a_color;}");
   let fs:WebGLShader|null=null;try{fs=shader(gl,gl.FRAGMENT_SHADER,"#version 300 es\nprecision mediump float;in vec3 v_color;out vec4 frag;void main(){frag=vec4(v_color,1.0);}");program=gl.createProgram();if(!program)throw Error("WebGL program allocation failed");gl.attachShader(program,vs);gl.attachShader(program,fs);gl.linkProgram(program);if(!gl.getProgramParameter(program,gl.LINK_STATUS))throw Error("WebGL mesh link failed");}finally{gl.deleteShader(vs);if(fs)gl.deleteShader(fs)}
   uniform=gl.getUniformLocation(program,"u_matrix");vao=gl.createVertexArray();position=gl.createBuffer();color=gl.createBuffer();if(!uniform||!vao||!position||!color)throw Error("WebGL mesh buffer allocation failed");
   gl.bindVertexArray(vao);gl.bindBuffer(gl.ARRAY_BUFFER,position);gl.bufferData(gl.ARRAY_BUFFER,mesh.vertices,gl.STATIC_DRAW);gl.enableVertexAttribArray(0);gl.vertexAttribPointer(0,3,gl.FLOAT,false,0,0);
   gl.bindBuffer(gl.ARRAY_BUFFER,color);gl.bufferData(gl.ARRAY_BUFFER,mesh.colors,gl.STATIC_DRAW);gl.enableVertexAttribArray(1);gl.vertexAttribPointer(1,3,gl.FLOAT,false,0,0);gl.bindVertexArray(null);gl.bindBuffer(gl.ARRAY_BUFFER,null);
  }catch(error){release(gl);throw error}},
  render(gl,args){if(!program||!vao||!uniform)return;const m=args.defaultProjectionData.mainMatrix,[x,y,z]=mesh.origin,translated=new Float32Array(m);
   for(let i=0;i<4;i++)translated[12+i]=Number(m[12+i])+Number(m[i])*x+Number(m[4+i])*y+Number(m[8+i])*z;
   gl.useProgram(program);gl.uniformMatrix4fv(uniform,false,translated);gl.bindVertexArray(vao);const cull=gl.isEnabled(gl.CULL_FACE);if(cull)gl.disable(gl.CULL_FACE);
   gl.drawArrays(gl.TRIANGLES,0,mesh.vertices.length/3);if(cull)gl.enable(gl.CULL_FACE);gl.bindVertexArray(null);
  },onRemove(_map,gl){release(gl)}};
}
