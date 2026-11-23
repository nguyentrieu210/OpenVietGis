(() => {
  "use strict";
  const byId = id => document.getElementById(id);
  const base = new URL(".", location.href).href;
  let sdk, active = null, busy = false, generation = 0;
  const status = text => { byId("status").textContent = text; };
  const activity = text => { byId("activity").textContent = text; };
  async function request(path, {method="GET", token, payload}={}) {
    const headers = {};
    if (token) headers.Authorization = "Bearer " + token;
    if (payload !== undefined) headers["Content-Type"] = "application/json";
    const response = await fetch(new URL(path,base), {method,headers,body:payload===undefined?undefined:JSON.stringify(payload),
      credentials:"omit",cache:"no-store",redirect:"error"});
    if (!response.ok) throw Error("Bridge không khả dụng ("+response.status+")");
    return response.json();
  }
  function reset(text) {
    ++generation; active=null; busy=false;
    byId("paircode").value=""; byId("pairing").hidden=true;
    byId("disconnect").hidden=true; byId("connect").hidden=false;
    byId("connect").disabled=!sdk; status(text);
  }
  async function stop() {
    const s=active; reset("Đã ngắt kết nối.");
    if(!s)return;
    sdk?.close();
    // Never wait indefinitely on a relay response before invalidating host requests.
    const close=request("sessions/"+s.session_id+"/close",{method:"POST",token:s.browser_token}).catch(()=>{});
    await Promise.race([close,new Promise(resolve=>setTimeout(resolve,500))]);
    location.reload();
  }
  async function heartbeat(s,g) {
    if(active!==s||generation!==g)return;
    try {
      const {command}=await request("sessions/"+s.session_id+"/poll",{method:"POST",token:s.browser_token,payload:{}});
      if(active!==s||generation!==g)return;
      if(command&&!busy){
        busy=true;activity("AI yêu cầu: "+command.method);
        const ackPath="sessions/"+s.session_id+"/commands/"+command.id+"/ack";
        let ok=false, result=null;
        try {
          if(command.expires_in<3)throw Error("Lệnh đã hết hạn");
          if(command.method==="map_view")result=await sdk.getMapView();
          else if(command.method==="map_selection")result=await sdk.getSelection();
          else if(command.method==="map_navigate"){await sdk.navigateMap(command.params);result=null}
          else throw Error("Không hỗ trợ");
          ok=true;
        }catch{}
        if(active!==s||generation!==g)return;
        try{await request(ackPath,{method:"POST",token:s.browser_token,payload:{ok,result}});activity(ok?"Đã thực hiện lệnh AI.":"Đã từ chối hoặc không thể thực hiện.")}catch{activity("Lệnh hết hạn hoặc kết nối đã mất.")}
        busy=false;
      }
    }catch{if(active===s){reset("Phiên hết hạn hoặc không còn kết nối.");sdk?.close();location.reload()}}
  }
  // Independent heartbeat keeps presence alive while authoritative host consent is pending.
  let polling = false;
  setInterval(async()=>{
    if(!active)return;
    const s=active,g=generation;
    if(busy){try{await request("sessions/"+s.session_id+"/ping",{method:"POST",token:s.browser_token,payload:{}})}catch{if(active===s){reset("Phiên đã hết hạn.");sdk?.close();location.reload()}}return}
    if(polling)return;polling=true;try{await heartbeat(s,g)}finally{polling=false}
  },600);
  byId("connect").onclick=async()=>{
    byId("connect").disabled=true;
    try{
      if(!sdk||!["map_view","map_selection","map_navigate"].every(x=>sdk.capabilities.includes(x)))throw Error("Plugin chưa có quyền bản đồ");
      const s=await request("sessions",{method:"POST",payload:{}});
      active=s;++generation;status("Đã tạo phiên ghép nối · 15 phút");
      byId("paircode").value=JSON.stringify({OVG_BRIDGE_BASE:base.replace(/\/$/,""),OVG_BRIDGE_SESSION:s.session_id,OVG_BRIDGE_TOKEN:s.ai_token},null,2);
      byId("pairing").hidden=false;byId("disconnect").hidden=false;byId("connect").hidden=true;
    }catch(e){reset(e.message)}
  };
  byId("disconnect").onclick=stop;
  addEventListener("pagehide",()=>{active=null;++generation;sdk?.close()});
  (async()=>{
    try {const config=await request("config");sdk=await OpenVietGISPartner.connect({hostOrigin:config.host_origin});status("GIS đã sẵn sàng");byId("connect").disabled=false}
    catch{status("Không kết nối được GIS. Mở plugin trong dự án đã cấp quyền.")}
  })();
})();
