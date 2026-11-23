import {useEffect,useRef,useState} from "react";
import "./swarm-landing.css";

type Point={x:number;y:number};
type Rect={x:number;y:number;w:number;h:number};
type Job={el:HTMLElement|SVGElement;kind:"box"|"trace"|"point";rect:Rect;path:Point[];delay:number;duration:number;color:string;role:string;apply:(p:number)=>void;restore:()=>void;done:boolean};
type Cursor={job:Job;start:Point;offset:number;label:string;rank:number};
const groups=[
 {section:".landing-hero",targets:".landing-kicker, .hero-copy h1, .hero-copy > p, .hero-proof, .showcase-header, .showcase-sidebar, .showcase-status, .showcase-float, .showcase-inspector"},
 {section:".landing-features",targets:".section-copy > div, .section-copy > p, .landing-feature"},
 {section:".landing-workflow",targets:".workflow-copy, .workflow-card, .workflow-connector"},
 {section:".landing-cta",targets:".cta-content, .cta-actions"},
];
const clamp=(x:number)=>Math.max(0,Math.min(1,x));
const ease=(x:number)=>x*x*(3-2*x);
const mix=(a:number,b:number,t:number)=>a+(b-a)*t;
const hash=(n:number)=>{const x=Math.sin(n*127.1+17.9)*43758.5453;return x-Math.floor(x)};
const pointAt=(points:Point[],t:number):Point=>{const a=clamp(t)*(points.length-1),i=Math.floor(a),p=points[i]!,q=points[Math.min(points.length-1,i+1)]!;return{x:mix(p.x,q.x,a-i),y:mix(p.y,q.y,a-i)}};
const rectOf=(el:Element):Rect=>{const r=el.getBoundingClientRect();return{x:r.left,y:r.top,w:r.width,h:r.height}};
const onScreen=(r:Rect)=>r.w>=0&&r.h>=0&&r.x+r.w>0&&r.y+r.h>0&&r.x<innerWidth&&r.y<innerHeight;
const screenPoint=(el:SVGGraphicsElement,x:number,y:number):Point=>{const m=el.getScreenCTM();return m?{x:x*m.a+y*m.c+m.e,y:x*m.b+y*m.d+m.f}:{x,y}};
function boxJob(el:HTMLElement,delay:number,duration:number,color="#5ee6b2"):Job{
 const r=rectOf(el),old=el.style.getPropertyValue("--swarm-reveal");
 el.classList.add("swarm-building");el.style.setProperty("--swarm-reveal","0%");
 return {el,kind:"box",rect:r,path:[{x:r.x,y:r.y+Math.min(16,r.h*.2)},{x:r.x+r.w,y:r.y+Math.min(16,r.h*.2)}],delay,duration,color,role:"UI",done:false,
 apply:p=>el.style.setProperty("--swarm-reveal",Math.round(p*1000)/10+"%"),
 restore:()=>{el.classList.remove("swarm-building");if(old)el.style.setProperty("--swarm-reveal",old);else el.style.removeProperty("--swarm-reveal")}};
}
function traceJob(el:SVGPathElement,delay:number,duration:number,color:string,role:string):Job|null{
 let length:number;try{length=el.getTotalLength()}catch{return null}if(!length)return null;
 const r=rectOf(el),path=Array.from({length:33},(_,i)=>{const p=el.getPointAtLength(i/32*length);return screenPoint(el,p.x,p.y)});
 const dash=el.style.strokeDasharray,offset=el.style.strokeDashoffset,fill=el.style.fillOpacity;
 const hasFill=getComputedStyle(el).fill!=="none";
 el.style.strokeDasharray=length+" "+length;el.style.strokeDashoffset=String(length);if(hasFill)el.style.fillOpacity="0";
 return{el,kind:"trace",rect:r,path,delay,duration,color,role,done:false,apply:p=>{el.style.strokeDashoffset=String(length*(1-p));if(hasFill)el.style.fillOpacity=String(clamp((p-.68)/.32))},
 restore:()=>{el.style.strokeDasharray=dash;el.style.strokeDashoffset=offset;el.style.fillOpacity=fill}};
}
function pointJob(el:SVGCircleElement,delay:number):Job{
 const r=rectOf(el),p=screenPoint(el,el.cx.baseVal.value,el.cy.baseVal.value),opacity=el.style.opacity,original=Number(getComputedStyle(el).opacity)||1;el.style.opacity="0";
 return{el,kind:"point",rect:r,path:[p,p],delay,duration:550,color:"#78c5ff",role:"Survey",done:false,apply:t=>el.style.opacity=String(original*t),restore:()=>{el.style.opacity=opacity}};
}
const steps=[{at:0,label:"Khởi tạo đội khảo sát"},{at:850,label:"Vẽ khung & dựng lớp dữ liệu"},{at:1700,label:"Số hóa ranh giới"},{at:2850,label:"Kết nối tuyến đường"},{at:3650,label:"Định vị điểm khảo sát"},{at:4800,label:"Hoàn thiện workspace"}];

export function LandingSwarm({enabled=true}:{enabled?:boolean}){
 const canvas=useRef<HTMLCanvasElement>(null),skip=useRef<()=>void>(()=>{}),autoPlayed=useRef(false);
 const [playing,setPlaying]=useState(false);
 useEffect(()=>{
  const cv=canvas.current,root=cv?.closest(".landing-page");if(!cv||!root)return;
  const context=cv.getContext("2d");if(!context)return;const ctx=context;
  // The public landing plays the same cinematic introduction at its canonical URL.
  // swarm=0 is an explicit opt-out for visitors who want a static experience.
  const media=matchMedia("(prefers-reduced-motion: reduce)"),force=new URLSearchParams(location.search).get("swarm")!=="0";
  const motionDisabled=()=>!force;
  cv.dataset.force=String(force);root.classList.toggle("swarm-force",force);
  let raf=0,scheduled=0,scrollTimer=0,alive=true,active=false,armed=false,w=0,h=0;
  let jobs:Job[]=[],seen=new Set<number>();
  const clear=()=>{jobs.forEach(j=>j.restore());jobs=[];ctx.clearRect(0,0,w,h);cv.dataset.active="false";delete cv.dataset.phase;root.classList.remove("swarm-active")};
  const finish=()=>{cancelAnimationFrame(raf);active=false;clear();if(alive)setPlaying(false)};
  const stop=()=>{armed=false;clearTimeout(scrollTimer);cancelAnimationFrame(scheduled);finish()};
  skip.current=stop;
  const size=()=>{w=innerWidth;h=innerHeight;const dpr=Math.min(devicePixelRatio||1,1.5);cv.width=Math.round(w*dpr);cv.height=Math.round(h*dpr);ctx.setTransform(dpr,0,0,dpr,0,0)};
  function drawCursor(p:Point,color:string,label:string,opacity:number,showLabel:boolean,angle:number){
   const x=Math.max(0,Math.min(w-16,p.x)),y=Math.max(0,Math.min(h-24,p.y));
   ctx.save();ctx.globalAlpha=opacity;ctx.translate(x,y);ctx.rotate(angle);
   ctx.beginPath();ctx.moveTo(0,0);ctx.lineTo(3.5,18);ctx.lineTo(7.5,12);ctx.lineTo(15,10.5);ctx.closePath();ctx.shadowColor=color;ctx.shadowBlur=5;ctx.strokeStyle="#071d29";ctx.lineWidth=1.8;ctx.fillStyle=color;ctx.stroke();ctx.fill();ctx.shadowBlur=0;ctx.rotate(-angle);
   if(showLabel){ctx.font="600 10px system-ui";const tw=ctx.measureText(label).width;ctx.fillStyle=color;ctx.beginPath();ctx.roundRect(10,15,tw+12,17,3);ctx.fill();ctx.fillStyle="#082329";ctx.fillText(label,16,27)}
   ctx.restore();
  }
  const start=(index:number)=>{
   if(!alive||motionDisabled()||!enabled||document.hidden)return;
   finish();size();const section=root.querySelector(groups[index]!.section);if(!section)return;
   const els=Array.from(section.querySelectorAll<HTMLElement>(groups[index]!.targets)).filter(el=>onScreen(rectOf(el)));
   if(index===0){
    const pathGroups=[["boundary","#63e7ba","Boundary",1500],["road","#ffd38b","Network",2660]] as const;
    for(const [layer,color,role,delay] of pathGroups){Array.from(section.querySelectorAll<SVGPathElement>('[data-swarm-layer="'+layer+'"] > path')).forEach((el,i)=>{if(!onScreen(rectOf(el)))return;const j=traceJob(el,delay+i*135,1550,color,role);if(j)jobs.push(j)})}
    Array.from(section.querySelectorAll<SVGCircleElement>('[data-swarm-layer="point"] > circle')).forEach((el,i)=>{if(onScreen(rectOf(el)))jobs.push(pointJob(el,3450+i*19))});
   }
   els.forEach((el,i)=>{const delay=index===0?(el.matches(".showcase-inspector,.showcase-float,.showcase-status")?4200+i*35:700+i*150):400+i*110;jobs.push(boxJob(el,delay,index===0?1120:900,index===0?"#baf5dc":"#0c9173"))});
   if(!jobs.length)return;
   seen.add(index);active=true;cv.dataset.active="true";root.classList.add("swarm-active");setPlaying(true);
   const count=index===0?(w<700?24:100):(w<700?12:24);cv.dataset.cursorCount=String(count);cv.dataset.traceCount=String(jobs.filter(j=>j.kind==="trace").length);
   const sorted=[...jobs.filter(j=>j.kind==="trace"),...jobs.filter(j=>j.kind==="box"),...jobs.filter(j=>j.kind==="point")];
   const agents:Cursor[]=Array.from({length:count},(_,i)=>{const job=sorted[i%sorted.length]!,rank=Math.floor(i/sorted.length),edge=hash(i+3);return{job,rank,start:{x:i%5===0?(i%2===0?20:w-20):w*edge,y:30+hash(i+119)*(h-50)},offset:hash(i+52)*500,label:job.role+" "+String(i+1).padStart(2,"0")}});
   const end=Math.max(...jobs.map(j=>j.delay+j.duration))+(index===0?850:500);
   const speed=index===0 ? 0.60 : 0.82;cv.dataset.durationMs=String(Math.round(end/speed));
   const t0=performance.now();let lastPhase=-1;
   const frame=(now:number)=>{
    if(!active)return;const t=(now-t0)*speed;ctx.clearRect(0,0,w,h);
    if(index===0){let k=0;for(let i=0;i<steps.length;i++)if(t>=steps[i]!.at)k=i;if(k!==lastPhase){lastPhase=k;cv.dataset.phase=String(k)}}
    for(const job of jobs){
     const p=clamp((t-job.delay)/job.duration),r=job.rect;
     if(p>=1){if(!job.done){job.done=true;job.restore()}continue}
     if(p>0){job.apply(job.kind==="trace"?p:ease(p));const tip=pointAt(job.path,p);
      if(job.kind==="box"){ctx.save();ctx.globalAlpha=.64;ctx.strokeStyle=job.color;ctx.lineWidth=1;ctx.setLineDash([3,4]);ctx.strokeRect(r.x,r.y,r.w*ease(p),r.h);ctx.setLineDash([]);ctx.fillStyle=job.color;ctx.fillRect(r.x+r.w*ease(p)-2,r.y-2,4,4);ctx.restore()}
      if(job.kind==="trace"){ctx.save();ctx.strokeStyle=job.color;ctx.globalAlpha=.85;ctx.lineWidth=1.5;ctx.beginPath();ctx.arc(tip.x,tip.y,5+p*3,0,Math.PI*2);ctx.stroke();ctx.restore()}
      if(job.kind==="point"){ctx.save();ctx.globalAlpha=.75*(1-p);ctx.strokeStyle=job.color;ctx.beginPath();ctx.arc(tip.x,tip.y,3+p*15,0,Math.PI*2);ctx.stroke();ctx.restore()}
     }
    }
    agents.forEach((a,i)=>{
     const j=a.job,p=clamp((t-j.delay)/j.duration),start=pointAt(j.path,0),target=pointAt(j.path,p),approach=clamp((t-a.offset)/Math.max(320,j.delay-a.offset)),exit=clamp((t-j.delay-j.duration)/650);
     const q=ease(approach),curve=Math.sin(q*Math.PI)*(hash(i+23)-.5)*95;
     let x=mix(a.start.x,start.x,q)+curve,y=mix(a.start.y,start.y,q)-Math.sin(q*Math.PI)*35;
     if(approach>=1){x=target.x+(a.rank?Math.sin(i)*12:0);y=target.y+(a.rank?Math.cos(i)*12:0)}
     if(exit>0){x+=Math.sin(i*2.399)*exit*70;y-=exit*45}
     const opacity=clamp(t/160)*(1-exit);
     if(opacity>.02)drawCursor({x,y},j.color,a.label,opacity,i%7===0||j.kind==="trace"&&a.rank===0,Math.sin(t*.003+i)*.08);
    });
    if(t<end)raf=requestAnimationFrame(frame);else finish();
   };raf=requestAnimationFrame(frame);
  };
  const next=()=>{if(!armed||active)return;for(let i=0;i<groups.length;i++){if(seen.has(i))continue;const el=root.querySelector(groups[i]!.section),r=el?.getBoundingClientRect();if(r&&r.top<innerHeight*.8&&r.bottom>innerHeight*.18){start(i);return}}};
  const onScroll=()=>{if(active)finish();if(armed){clearTimeout(scrollTimer);scrollTimer=window.setTimeout(next,180)}};
  const onInput=(e:Event)=>{if(e.target instanceof Element&&e.target.closest("button,a,input,dialog,[role=button]")&&!e.target.closest(".swarm-skip"))stop()};
  const onKey=(e:KeyboardEvent)=>{if(e.key==="Tab"||e.key==="Escape")stop()};
  const onResize=()=>{if(active)finish()};
  const onMotion=()=>{if(motionDisabled())stop()};
  const onVisibility=()=>{if(document.hidden){finish()}else if(armed&&!seen.has(0)){scheduled=requestAnimationFrame(next)}};
  if(enabled&&!motionDisabled()&&!autoPlayed.current){autoPlayed.current=true;armed=true;scheduled=requestAnimationFrame(next)}
  window.addEventListener("scroll",onScroll,{passive:true});window.addEventListener("resize",onResize);root.addEventListener("pointerdown",onInput,true);window.addEventListener("keydown",onKey);document.addEventListener("visibilitychange",onVisibility);media.addEventListener("change",onMotion);
  return()=>{alive=false;stop();root.classList.remove("swarm-force");window.removeEventListener("scroll",onScroll);window.removeEventListener("resize",onResize);root.removeEventListener("pointerdown",onInput,true);window.removeEventListener("keydown",onKey);document.removeEventListener("visibilitychange",onVisibility);media.removeEventListener("change",onMotion)};
 },[enabled]);
 return <><canvas aria-hidden="true" ref={canvas} className="landing-swarm-canvas" data-active="false"/>{enabled&&playing&&<button className="swarm-skip" type="button" onClick={()=>skip.current()}>Bỏ qua</button>}</>;
}
