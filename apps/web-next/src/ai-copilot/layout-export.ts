import type {Map as MlMap} from "maplibre-gl";
import {captureFrame,svgCanvas,pdfFromCanvas,pngFromCanvas,saveBlob,type Camera,type Shot} from "../LayoutRender";
type Item={id:string;kind:"map"|"text"|"legend"|"north"|"scale"|"rect";x:number;y:number;w:number;h:number;text:string;font_size:number;color:string;border:boolean;grid:boolean;camera?:Camera;map_id?:string};
type Doc={version:1;paper:"A0"|"A1"|"A2"|"A3"|"A4";landscape:boolean;dpi:150|300;items:Item[]};
type Layer={name:string;color:string};
const papers={A0:[841,1189],A1:[594,841],A2:[420,594],A3:[297,420],A4:[210,297]};
const ns="http://www.w3.org/2000/svg";
function element(tag:string,attrs:Record<string,string|number>,text?:string){const e=document.createElementNS(ns,tag);for(const [k,v] of Object.entries(attrs))e.setAttribute(k,String(v));if(text!==undefined)e.textContent=text;return e}
function lines(value:string,width:number,font:number){const max=Math.max(1,Math.floor(width/(font*.56)));return value.split("\n").flatMap(line=>{const out:string[]=[];let row="";for(const token of line.split(/\s+/)){if(row&&row.length+token.length+1>max){out.push(row);row=""}row+=(row?" ":"")+token;while(row.length>max){out.push(row.slice(0,max));row=row.slice(max)}}out.push(row);return out})}
/** Saved documents are already validated by Core. Recheck bounds before rendering. No arbitrary SVG or URLs. */
export async function renderAiLayout(map:MlMap,raw:unknown,format:"pdf"|"png",layers:Layer[],guard:()=>void){
 const d=raw as Doc;
 if(!d||d.version!==1||!Object.hasOwn(papers,d.paper)||![150,300].includes(d.dpi)||!Array.isArray(d.items)||d.items.length>40)throw Error("Invalid saved layout");
 const [pw,ph]=papers[d.paper],w=d.landscape?ph:pw,h=d.landscape?pw:ph;
 const maps=d.items.filter(i=>i.kind==="map");
 if(!maps.length||maps.length>4||(d.dpi===300&&!["A3","A4"].includes(d.paper)))throw Error("Unsupported paper or map frames");
 for(const i of d.items)if(![i.x,i.y,i.w,i.h,i.font_size].every(Number.isFinite)||i.x<0||i.y<0||i.w<5||i.h<5||i.x+i.w>w+.001||i.y+i.h>h+.001||!/^#[a-f0-9]{6}$/i.test(i.color))throw Error("Invalid layout geometry");
 const shots=new Map<string,Shot>();
 for(const i of maps){guard();if(!i.camera)throw Error("Missing camera");shots.set(i.id,await captureFrame(map,i.camera,i.w,i.h,d.dpi))}
 guard();
 const svg=element("svg",{xmlns:ns,viewBox:"0 0 "+w+" "+h,width:w+"mm",height:h+"mm"}) as SVGSVGElement;
 svg.append(element("rect",{width:w,height:h,fill:"white"}));
 for(const i of d.items){
   const g=element("g",{transform:"translate("+i.x+" "+i.y+")"});svg.append(g);
   const txt=(s:string,x:number,y:number,font=i.font_size,extra:Record<string,string|number>={})=>g.append(element("text",{x,y,"font-size":font,fill:i.color,"font-family":"Arial, sans-serif",...extra},s));
   if(i.kind==="map"){
     const shot=shots.get(i.id)!;g.append(element("image",{width:i.w,height:i.h,href:shot.url,preserveAspectRatio:"none"}));
     if(i.grid){const sub=element("svg",{width:i.w,height:i.h,viewBox:"0 0 1000 1000",preserveAspectRatio:"none",overflow:"hidden"});for(const l of shot.grid){sub.append(element("path",{d:l.path,stroke:"#173c40","stroke-width":1.3,"stroke-dasharray":"6 5",fill:"none"}));sub.append(element("text",{x:l.x,y:l.y,"font-size":12,stroke:"white","stroke-width":2,"paint-order":"stroke",fill:"#18393a"},l.label))}g.append(sub)}
     if(shot.attribution){g.append(element("rect",{y:Math.max(0,i.h-5),width:i.w,height:5,fill:"#fff",opacity:.88}));txt(shot.attribution,1,i.h-1.5,2.2,{textLength:Math.min(i.w-2,shot.attribution.length*1.1),lengthAdjust:"spacingAndGlyphs"})}
   }else if(i.kind==="text"){lines(i.text,i.w-3,i.font_size).slice(0,Math.max(1,Math.floor((i.h-2)/(i.font_size*1.2)))).forEach((s,n)=>txt(s,1.5,i.font_size+1+n*i.font_size*1.2))}
   else if(i.kind==="legend"){
     txt(i.text||"CHÚ GIẢI",1.5,i.font_size+1);let y=i.font_size+2;
     for(const l of layers){const t=lines(l.name,i.w-10,i.font_size),need=Math.max(6,t.length*i.font_size*1.2+2);if(y+need>i.h-1)break;g.append(element("rect",{x:2,y:y+1,width:5,height:3.5,fill:/^#[a-f0-9]{6}$/i.test(l.color)?l.color:"#517269",stroke:"#415a55","stroke-width":.2}));t.forEach((s,n)=>txt(s,9,y+4+n*i.font_size*1.2));y+=need}
   }else if(i.kind==="north"){
     const m=maps.find(x=>x.id===i.map_id),s=m&&shots.get(m.id),t=Math.min(i.w,i.h)*.6;
     const n=element("g",{transform:"translate("+i.w/2+" "+i.h/2+") rotate("+(-(s?.bearing??0))+")"});
     n.append(element("text",{y:-t*.7,"text-anchor":"middle","font-size":Math.min(5,t*.32),fill:i.color,"font-weight":"bold"},"N"),element("path",{d:"M0 "+(-t*.5)+" L"+t*.28+" "+t*.45+" L0 "+t*.22+" L"+(-t*.28)+" "+t*.45+" Z",fill:i.color}));g.append(n)
   }else if(i.kind==="scale"){
     const m=maps.find(x=>x.id===i.map_id),s=m&&shots.get(m.id);if(!m||!s)throw Error("Invalid scale link");
     const denom=Math.round(s.meters/(m.w/1000));txt("Tỷ lệ ≈ 1:"+denom.toLocaleString("vi-VN"),1.5,i.font_size+1);
     const raw=s.meters*i.w*.7/m.w;if(Number.isFinite(raw)&&raw>0){const power=10**Math.floor(Math.log10(raw)),v=([1,2,5,10].filter(n=>n*power<=raw).at(-1)??1)*power,bw=m.w*v/s.meters,by=i.font_size+3;g.append(element("rect",{x:2,y:by,width:bw,height:2.5,fill:"#fff",stroke:i.color,"stroke-width":.3}),element("rect",{x:2,y:by,width:bw/2,height:2.5,fill:i.color}));txt("0",2,by+5.5,2.6);txt(v>=1000?v/1000+" km":v+" m",2+bw,by+5.5,2.6,{"text-anchor":"end"})}
   }
   if(i.border||i.kind==="rect")g.append(element("rect",{width:i.w,height:i.h,fill:"none",stroke:i.color,"stroke-width":.4}));
 }
 const canvas=await svgCanvas(svg,w,h,d.dpi),blob=format==="pdf"?await pdfFromCanvas(canvas,w,h):await pngFromCanvas(canvas,d.dpi);
 guard();const filename="openvietgis-layout."+format;saveBlob(blob,filename);return {bytes:blob.size,filename};
}
