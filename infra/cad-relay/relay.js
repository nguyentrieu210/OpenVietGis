/* Dedicated CAD origin only. Never host this beside GIS authentication. */
(() => {
 "use strict";
 const parentOrigin=window.OVG_CAD_PARENT_ORIGIN, release="2026.46";
 const channel=location.hash.slice(1);
 const cap=["dwg","dxf","readOnly","save","saveId","dirty"];
 const MAX=25*1024*1024;
 const isOrigin=value=>{try{const u=new URL(value);return u.origin===value&&(u.protocol==="https:"||(u.protocol==="http:"&&u.hostname==="127.0.0.1"))}catch{return false}};
 if(!isOrigin(parentOrigin)||parentOrigin===location.origin||parent===self||!/^([a-f0-9]{32})$/.test(channel))return;
 const iframe=document.createElement("iframe");
 iframe.title="OpenCAD Studio";
 iframe.referrerPolicy="no-referrer";
 iframe.setAttribute("sandbox","allow-scripts allow-same-origin");
 let ready=false,opened=false,loaded=false,readOnly=true,filename="",format="",pending="";
 const bytes=b=>b instanceof ArrayBuffer&&b.byteLength>0&&b.byteLength<=MAX;
 const name=n=>typeof n==="string"&&n.length<=180&&/^[^/\\\x00-\x1f\x7f]+\.(dwg|dxf)$/i.test(n);
 const id=s=>typeof s==="string"&&/^save-[0-9]{1,20}$/.test(s);
 const toParent=(type,data={},transfer=[])=>parent.postMessage({source:"ovg-cad-relay",channel,type,...data},parentOrigin,transfer);
 const toEngine=(type,data={},transfer=[])=>iframe.contentWindow?.postMessage({source:"opencadstudio-host",type,...data},location.origin,transfer);
 const fail=code=>toParent("error",{code});
 window.addEventListener("message",e=>{
  const d=e.data;if(!d||typeof d!=="object")return;
  if(e.origin===parentOrigin&&e.source===parent&&d.source==="ovg-cad-host"&&d.channel===channel){
   if(d.type==="open"&&ready&&!opened&&name(d.name)&&bytes(d.buffer)&&typeof d.readOnly==="boolean"){
    opened=true;filename=d.name;format=filename.split(".").pop().toLowerCase();readOnly=d.readOnly;
    toEngine("ocs:open",{name:filename,buffer:d.buffer,readOnly},[d.buffer]);
   }else if(d.type==="request-save"&&loaded&&!readOnly&&!pending&&d.format===format){
    toEngine("ocs:request-save",{format});
   }else if(d.type==="saved"&&pending&&d.saveId===pending&&typeof d.ok==="boolean"){
    const saveId=pending;pending="";
    toEngine("ocs:saved",{saveId,ok:d.ok,name:filename,error:d.ok?"":"Project save failed"});
   }
   return;
  }
  if(e.origin!==location.origin||e.source!==iframe.contentWindow||d.source!=="opencadstudio-embed")return;
  if(d.type==="ocs:ready"&&!ready&&d.version===release&&Array.isArray(d.capabilities)&&cap.every(x=>d.capabilities.includes(x))){
   ready=true;toParent("ready",{version:release,capabilities:cap});
  }else if(d.type==="ocs:loaded"&&opened&&!loaded&&d.name===filename){
   loaded=true;toParent("loaded",{name:filename});
  }else if(d.type==="ocs:dirty"&&opened&&typeof d.dirty==="boolean"){
   if(readOnly&&d.dirty){fail("read_only");return}toParent("dirty",{dirty:d.dirty});
  }else if(d.type==="ocs:save"&&loaded&&id(d.saveId)){
   if(readOnly||pending||!bytes(d.buffer)||d.format!==format){
    toEngine("ocs:saved",{ok:false,saveId:d.saveId,error:"Invalid or concurrent save"});return;
   }
   pending=d.saveId;toParent("save",{saveId:pending,format,buffer:d.buffer},[d.buffer]);
  }else if(d.type==="ocs:error"){fail(typeof d.code==="string"?d.code.slice(0,80):"engine_error")}
 });
 iframe.src="/app/?embed=1";
 document.getElementById("engine").append(iframe);
})();
