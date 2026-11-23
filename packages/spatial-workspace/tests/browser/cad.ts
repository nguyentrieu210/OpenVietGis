import {prepareCadReadOnly} from "../../src/cad-embed";
const status=document.querySelector("#state")!,surface=document.querySelector<HTMLElement>("#surface")!,signal=new AbortController().signal;
prepareCadReadOnly(surface,{name:"fixture.dxf",bytes:new Uint8Array([1,2,3,4]).buffer,engineReviewed:true},signal,()=>{status.textContent="integrity failure"}).then(engine=>{engine.activate();status.textContent="ready (mock protocol)"}).catch(()=>{status.textContent="failed"});
