import * as ml from "maplibre-gl";
export type Camera={lng:number;lat:number;zoom:number;bearing:number;reference_width:number};
export type GridLine={path:string;label:string;x:number;y:number};
export type Shot={url:string;meters:number;bearing:number;ratio:number;grid:GridLine[];attribution:string};
export function currentCamera(m:ml.Map):Camera{const c=m.getCenter();return {lng:((c.lng+180)%360+360)%360-180,lat:Math.max(-85,Math.min(85,c.lat)),zoom:Math.max(0,Math.min(24,m.getZoom())),bearing:((m.getBearing()+180)%360+360)%360-180,reference_width:Math.max(1,m.getCanvas().clientWidth)}}
const clamp=(v:number,min:number,max:number)=>Math.max(min,Math.min(max,v));
export async function captureFrame(live:ml.Map,camera:Camera,wmm:number,hmm:number,dpi:number):Promise<Shot>{
 const rawW=Math.max(40,Math.round(wmm*dpi/25.4)),rawH=Math.max(40,Math.round(hmm*dpi/25.4)),factor=Math.min(1,4096/rawW,4096/rawH,Math.sqrt(8_000_000/(rawW*rawH)));
 const width=Math.max(40,Math.round(rawW*factor)),height=Math.max(40,Math.round(rawH*factor));
 const host=document.createElement("div");Object.assign(host.style,{position:"fixed",left:"-12000px",top:"0",width:width+"px",height:height+"px",pointerEvents:"none"});document.body.appendChild(host);
 let map:ml.Map|undefined;
 try{
  const style=live.getStyle();const attrs=new Set<string>();const active=new Set(style.layers.filter(l=>l.layout?.visibility!=="none"&&"source" in l).map(l=>(l as {source:string}).source));
  for(const [id,s] of Object.entries(style.sources))if(active.has(id)&&"attribution" in s&&typeof s.attribution==="string"){const decoded=new DOMParser().parseFromString(s.attribution,"text/html").body.textContent?.trim();if(decoded)attrs.add(decoded)}
  map=new ml.Map({container:host,style,center:[camera.lng,camera.lat],zoom:clamp(camera.zoom+Math.log2(width/camera.reference_width),0,24),bearing:camera.bearing,pitch:0,maxZoom:24,interactive:false,attributionControl:false,pixelRatio:1,canvasContextAttributes:{preserveDrawingBuffer:true,antialias:true}});
  const m=map;
  await new Promise<void>((resolve,reject)=>{let done=false;let issue="";const clean=()=>{clearTimeout(timer);m.off("idle",idle);m.off("error",err)};const idle=()=>{if(done)return;done=true;clean();if(issue)reject(Error("Một nguồn bản đồ không tải được: "+issue));else resolve()};const err=(e:ml.ErrorEvent)=>{issue=String(e.error?.message??"Lỗi nguồn bản đồ").slice(0,180)};const timer=setTimeout(()=>{if(done)return;done=true;clean();reject(Error("Quá thời gian tải bản đồ in. Thử thu nhỏ vùng hoặc đổi bản đồ nền."))},30000);m.on("error",err);m.on("idle",idle)});
  const cv=m.getCanvas(),url=cv.toDataURL("image/png"),a=m.unproject([0,height/2]),b=m.unproject([width,height/2]),meters=a.distanceTo(b),box=m.getBounds(),grid:GridLine[]=[];
  const span=Math.max(box.getEast()-box.getWest(),box.getNorth()-box.getSouth()),raw=span/5,power=10**Math.floor(Math.log10(raw)),step=([1,2,5,10].find(x=>x*power>=raw)??10)*power;
  const proj=(lng:number,lat:number)=>{const p=m.project([lng,lat]);return [p.x/width*1000,p.y/height*1000]};
  const line=(v:number,vertical:boolean)=>{const points=Array.from({length:15},(_,i)=>vertical?proj(v,box.getSouth()+(box.getNorth()-box.getSouth())*i/14):proj(box.getWest()+(box.getEast()-box.getWest())*i/14,v));const inside=points.find(([x,y])=>x>=0&&x<=1000&&y>=0&&y<=1000);if(!inside)return;grid.push({path:points.map((p,i)=>(i?"L":"M")+p.map(n=>n.toFixed(2)).join(",")).join(" "),label:v.toFixed(Math.min(6,Math.max(0,-Math.floor(Math.log10(step)))))+"°",x:clamp(inside[0]+5,5,930),y:clamp(inside[1]+12,14,990)})};
  if(Number.isFinite(step)&&step>0){for(let x=Math.ceil(box.getWest()/step)*step;x<=box.getEast()&&grid.length<20;x+=step)line(x,true);for(let y=Math.ceil(box.getSouth()/step)*step;y<=box.getNorth()&&grid.length<40;y+=step)line(y,false)}
  if(!url.startsWith("data:image/png")||!Number.isFinite(meters)||meters<=0)throw Error("Không lấy được ảnh / tỷ lệ bản đồ");
  return {url,meters,bearing:m.getBearing(),ratio:width/height,grid,attribution:[...attrs].join(" · ").slice(0,350)};
 }finally{map?.remove();host.remove()}
}
export async function svgCanvas(svg:SVGSVGElement,wmm:number,hmm:number,dpi:number){const clone=svg.cloneNode(true) as SVGSVGElement;clone.querySelectorAll("[data-editor]").forEach(n=>n.remove());clone.setAttribute("xmlns","http://www.w3.org/2000/svg");clone.setAttribute("width",String(wmm));clone.setAttribute("height",String(hmm));const url=URL.createObjectURL(new Blob([new XMLSerializer().serializeToString(clone)],{type:"image/svg+xml;charset=utf-8"}));try{const image=await new Promise<HTMLImageElement>((resolve,reject)=>{const i=new Image();i.onload=()=>resolve(i);i.onerror=()=>reject(Error("Không dựng được bản in SVG"));i.src=url});const c=document.createElement("canvas");c.width=Math.round(wmm*dpi/25.4);c.height=Math.round(hmm*dpi/25.4);if(c.width*c.height>40_000_000)throw Error("Trang vượt giới hạn 40 triệu pixel");const ctx=c.getContext("2d");if(!ctx)throw Error("Không tạo được ảnh in");ctx.fillStyle="#fff";ctx.fillRect(0,0,c.width,c.height);ctx.drawImage(image,0,0,c.width,c.height);return c}finally{URL.revokeObjectURL(url)}}
export function canvasBlob(c:HTMLCanvasElement,type:string,quality?:number){return new Promise<Blob>((resolve,reject)=>c.toBlob(b=>b?resolve(b):reject(Error("Không mã hóa được bản in")),type,quality))}
/** Embed the requested physical print resolution, not the browser's default. */
export async function pngFromCanvas(canvas:HTMLCanvasElement,dpi:number):Promise<Blob>{
 const png=new Uint8Array(await (await canvasBlob(canvas,"image/png")).arrayBuffer());
 const view=new DataView(png.buffer),chunks:Uint8Array[]=[png.slice(0,8)];
 const phys=new Uint8Array(21),v=new DataView(phys.buffer),ppm=Math.round(dpi/0.0254);
 v.setUint32(0,9);phys.set([112,72,89,115],4);v.setUint32(8,ppm);v.setUint32(12,ppm);phys[16]=1;
 let crc=0xffffffff;for(const b of phys.subarray(4,17)){crc^=b;for(let i=0;i<8;i++)crc=(crc>>>1)^((crc&1)?0xedb88320:0)}v.setUint32(17,(crc^0xffffffff)>>>0);
 for(let off=8;off+12<=png.length;){const len=view.getUint32(off),end=off+12+len;if(end>png.length)throw Error("PNG không hợp lệ");const kind=String.fromCharCode(...png.subarray(off+4,off+8));if(kind!=="pHYs")chunks.push(png.slice(off,end));if(kind==="IHDR")chunks.push(phys);off=end}
 return new Blob(chunks.map(a=>a.buffer.slice(a.byteOffset,a.byteOffset+a.byteLength) as ArrayBuffer),{type:"image/png"});
}
export async function pdfFromCanvas(canvas:HTMLCanvasElement,wmm:number,hmm:number):Promise<Blob>{
 const jpeg=new Uint8Array(await (await canvasBlob(canvas,"image/jpeg",.96)).arrayBuffer()),w=(wmm*72/25.4).toFixed(4),h=(hmm*72/25.4).toFixed(4),enc=new TextEncoder(),chunks:Uint8Array[]=[],offsets=[0];let offset=0;
 const push=(chunk:string|Uint8Array)=>{const a=typeof chunk==="string"?enc.encode(chunk):chunk;chunks.push(a);offset+=a.byteLength};
 const obj=(id:number,body:string)=>{offsets[id]=offset;push(id+" 0 obj\n"+body+"\nendobj\n")};
 push("%PDF-1.4\n");obj(1,"<< /Type /Catalog /Pages 2 0 R >>");obj(2,"<< /Type /Pages /Kids [3 0 R] /Count 1 >>");obj(3,"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 "+w+" "+h+"] /Resources << /XObject << /Im0 4 0 R >> >> /Contents 5 0 R >>");
 offsets[4]=offset;push("4 0 obj\n<< /Type /XObject /Subtype /Image /Width "+canvas.width+" /Height "+canvas.height+" /ColorSpace /DeviceRGB /BitsPerComponent 8 /Filter /DCTDecode /Length "+jpeg.length+" >>\nstream\n");push(jpeg);push("\nendstream\nendobj\n");
 const stream="q\n"+w+" 0 0 "+h+" 0 0 cm\n/Im0 Do\nQ\n";obj(5,"<< /Length "+enc.encode(stream).length+" >>\nstream\n"+stream+"endstream");const xref=offset;push("xref\n0 6\n0000000000 65535 f \n");for(let i=1;i<=5;i++)push(String(offsets[i]).padStart(10,"0")+" 00000 n \n");push("trailer\n<< /Size 6 /Root 1 0 R >>\nstartxref\n"+xref+"\n%%EOF\n");return new Blob(chunks.map(a=>a.buffer.slice(a.byteOffset,a.byteOffset+a.byteLength) as ArrayBuffer),{type:"application/pdf"})
}
export function saveBlob(blob:Blob,name:string){const url=URL.createObjectURL(blob);const a=document.createElement("a");a.href=url;a.download=name;a.click();setTimeout(()=>URL.revokeObjectURL(url),1500)}
