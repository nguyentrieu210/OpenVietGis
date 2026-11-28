import {modeAllows,validSession,pinCommand,assertCommandCurrent,recordHistory} from "./session-guard.js";
/** First-party panel: mount inside a trusted, authenticated GIS workspace. */
export function mountAiCopilot(host, {organizationId, projectId, getBridge,getMode=()=>"gis",getContextEpoch=()=>0,runCad,requestAgent}) {
  // Explicit session consent is limited to these read-only / temporary commands.
  // All downloads, persisted writes, administration and unknown tools are reviewed individually.
  const SESSION_SAFE=new Set(["map_view","map_selection","map_navigate","gis_layers","gis_features","gis_feature","gis_query","gis_geometry_preview","gis_topology","gis_overlay","gis_layer_visibility","gis_select","gis_raster_layers","gis_processing_job","gis_models","gis_layouts","gis_plugins","gis_plugin_records"]);
  const node=document.createElement("section");
  node.className="ovg-ai-copilot";
  node.innerHTML='<button type="button" data-ai="toggle"><span aria-hidden="true">✦</span> AI Copilot</button><div data-ai="panel" hidden><header data-ai="header"><div><span data-ai="eyebrow">OPENVIETGIS · AI WORKSPACE</span><h2>Trợ lý bản đồ AI</h2><p>Điều khiển và phân tích GIS bằng ngôn ngữ tự nhiên.</p></div><button type="button" data-ai="dismiss" aria-label="Thu gọn AI Copilot">×</button></header><div data-ai="connection"><p data-ai="status" role="status">Chưa kết nối</p><small data-ai="scope"></small><small data-ai="expiry"></small><button type="button" data-ai="connect">Kết nối AI</button><button type="button" data-ai="close" hidden>Ngắt kết nối</button></div><div data-ai="consent" hidden><label><input type="checkbox" data-ai="auto"> <strong>Luôn cho phép lệnh an toàn trong phiên</strong></label><p>Đọc dữ liệu, phân tích xem trước, di chuyển bản đồ và chọn đối tượng. Có thể tắt bất cứ lúc nào. Tạo, sửa, xóa, xuất dữ liệu và quản trị luôn hỏi riêng.</p></div><div data-ai="review" hidden role="region" aria-label="Xác nhận lệnh AI"><span data-ai="risk"></span><h3 data-ai="command"></h3><p data-ai="explain"></p><details data-ai="detailbox" hidden><summary>Thông số và đối tượng liên quan</summary><pre data-ai="details"></pre></details><div data-ai="actions"><button type="button" data-ai="always" hidden>Luôn cho phép trong phiên</button><button type="button" data-ai="accept">Cho phép lần này</button><button type="button" data-ai="deny">Từ chối</button></div></div><div data-ai="native" hidden><strong>AI Agent tích hợp</strong><div data-ai="acp" hidden><label>Coding Agent ACP <select data-ai="acp-agent"></select></label><label><input type="checkbox" data-ai="acp-consent"> Gửi riêng yêu cầu văn bản đến runner ACP; không chia sẻ dữ liệu GIS hay ảnh.</label><button type="button" data-ai="acp-run">Chạy Coding Agent</button></div><label>Nhà cung cấp <select data-ai="provider"></select></label><label>Kỹ năng <select data-ai="skill"><option value="">Tự động</option><option value="explore-data">Khám phá dữ liệu</option><option value="geoprocessing">Phân tích GIS</option><option value="thematic-map">Bản đồ chuyên đề</option><option value="print-layout">In bản đồ</option><option value="add-data">Nhập dữ liệu</option><option value="share-web-map">Xuất bản đồ</option></select></label><textarea data-ai="task" maxlength="16000" rows="3" placeholder="Nhập yêu cầu GIS..." aria-label="Yêu cầu AI"></textarea><label><input type="checkbox" data-ai="provider-consent"> Cho phép gửi yêu cầu và dữ liệu GIS được duyệt đến nhà cung cấp đã chọn trong lượt này.</label><label><input type="checkbox" data-ai="writes"> Cho phép đề xuất lệnh ghi (vẫn duyệt riêng)</label><label><input type="checkbox" data-ai="multi-agent"> Dùng đội chuyên gia AI (tối đa 3, chỉ đọc)</label><div data-ai="vision" hidden><button type="button" data-ai="capture">Chụp ảnh bản đồ</button><button type="button" data-ai="remove-image" hidden>Bỏ ảnh</button><img data-ai="preview" alt="Ảnh bản đồ gửi cho AI" hidden><small data-ai="image-status" role="status"></small><label><input type="checkbox" data-ai="vision-consent"> Đồng ý gửi đúng ảnh bản đồ xem trước đến mô hình đã chọn.</label></div><div><button type="button" data-ai="run">Thực thi</button><button type="button" data-ai="cancel" hidden>Hủy / đóng phiên</button></div><pre data-ai="answer" role="status"></pre></div><details data-ai="history"><summary>Lịch sử lệnh trong phiên mở trang</summary><ol data-ai="history-items"></ol><p>Không lưu thông số, dữ liệu trả về hay token.</p></details><footer data-ai="footer"><strong>Kết nối ChatGPT / Claude</strong><p>Đăng nhập cùng tài khoản GIS, chọn dự án rồi kết nối. Phiên có hiệu lực tối đa 15 phút.</p><code data-ai="endpoint"></code></footer></div>';
  host.append(node);
  const get=name=>node.querySelector('[data-ai="'+name+'"]');
  get("endpoint").textContent=location.origin+"/mcp";
  let acpChoices=[];
  let providerChoices=[],nativeRun=null,nativeJob=null,image=null,capturing=false;
  let active=null,command=null,epoch=0,disposed=false,polling=false,submitting=false,autoAllowed=false,connecting=false,history=[],seen=new Set(),lastContextEpoch=getContextEpoch();
  const context=()=>{const mode=getMode();let cad="";if(mode==="cad"&&typeof runCad==="function"){try{const state=runCad("cad_state",{});cad=JSON.stringify([state.document_id,state.version,state.dirty,state.saving])}catch{cad="unavailable"}}return {mode,epoch:getContextEpoch(),cad}};
  const note=(c,label)=>{history=recordHistory(history,{id:c.id,method:c.method,mode:c.scope?.mode??getMode(),status:label,at:Date.now()});const list=get("history-items");list.replaceChildren(...history.map(item=>{const li=document.createElement("li");li.textContent=new Date(item.at).toLocaleTimeString("vi-VN")+" · "+item.mode.toUpperCase()+" · "+item.method+" · "+item.status;return li}))};
  const updateScope=()=>{get("scope").textContent="Tổ chức "+organizationId+" · Dự án "+projectId+" · "+getMode().toUpperCase();get("expiry").textContent=active?"Phiên còn "+Math.max(0,Math.ceil((active.deadline-performance.now())/1000))+" giây":"";};
  updateScope();
  function setAutomatic(value){
    autoAllowed=!!value&&!!active;
    get("auto").checked=autoAllowed;
  }
  const status=t=>{get("status").textContent=t;};
  const show=(k,flag)=>{get(k).hidden=!flag;};
  const setReview=(cmd)=>{
    command=cmd;show("review",!!cmd);
    if(cmd){
      show("panel",true);
      const names={cad_state:"Đọc trạng thái bản vẽ CAD",cad_select_handle:"Chọn Handle trên đúng bản vẽ CAD",map_view:"Chia sẻ vùng nhìn bản đồ",map_selection:"Chia sẻ hình học đang chọn",map_navigate:"Di chuyển và zoom bản đồ",gis_layers:"Đọc danh sách lớp",gis_features:"Chia sẻ đối tượng và thuộc tính",gis_feature:"Đọc một đối tượng",gis_query:"Truy vấn thuộc tính / không gian",gis_geometry_preview:"Tính hình học CAD/phân tích",gis_topology:"Kiểm tra topology",gis_overlay:"Vẽ lớp xem trước",gis_layer_visibility:"Hiện/ẩn lớp",gis_select:"Chọn đối tượng",gis_changeset:"GHI DỮ LIỆU: tạo/sửa/xóa đối tượng",gis_revert:"GHI DỮ LIỆU: hoàn tác changeset",gis_create_layer:"GHI DỮ LIỆU: tạo lớp",gis_schema:"GHI DỮ LIỆU: thay đổi cấu trúc trường",gis_export:"Xuất GeoJSON",gis_raster_layers:"Đọc lớp raster",gis_raster_process:"GHI DỮ LIỆU: xử lý raster",gis_raster_report:"Báo cáo raster",gis_raster_download:"Tải GeoTIFF",gis_processing_run:"GHI DỮ LIỆU: chạy xử lý vector",gis_processing_job:"Đọc trạng thái processing",gis_models:"Đọc mô hình xử lý",gis_model_save:"GHI DỮ LIỆU: lưu mô hình",gis_model_run:"GHI DỮ LIỆU: chạy toàn bộ mô hình",gis_layouts:"Đọc layout",gis_layout_save:"GHI DỮ LIỆU: lưu layout",gis_layout_export:"Xuất layout PDF/PNG/JSON",gis_plugins:"Đọc danh mục plugin",gis_plugin_activate:"QUẢN TRỊ: bật plugin",gis_plugin_disable:"QUẢN TRỊ: tắt plugin",gis_plugin_records:"Đọc bản ghi plugin",gis_plugin_create:"GHI DỮ LIỆU: tạo bản ghi plugin",gis_plugin_update:"GHI DỮ LIỆU: sửa bản ghi plugin"};
      get("command").textContent=names[cmd.method]||"Lệnh không hỗ trợ";
      const safe=SESSION_SAFE.has(cmd.method);
      get("risk").textContent=safe?"LỆNH AN TOÀN · KHÔNG GHI DỮ LIỆU":"CẦN XÁC NHẬN RIÊNG";
      get("risk").dataset.safe=String(safe);
      get("explain").textContent=safe?"AI yêu cầu thao tác đọc hoặc xem trước trong dự án đang mở. Không thay đổi dữ liệu lưu trữ.":"Thao tác có thể thay đổi dữ liệu, tải tệp hoặc quản trị dự án. Kiểm tra thông số trước khi cho phép.";
      show("always",safe);
      const details=get("details");details.textContent=JSON.stringify({organization_id:organizationId,project_id:projectId,mode:cmd.scope.mode,...(cmd.scope.cad?{cad_context:cmd.scope.cad}:{}),params:cmd.params??{}},null,2);
      show("detailbox",true);
      get("detailbox").open=!safe;
    }
  };
  async function req(path,session,payload,keepalive=false){
    const headers={"X-OVG-AI":"1","Content-Type":"application/json"};
    if(session)headers["X-OVG-Bridge"]=session.browser_token;
    const r=await fetch("/ai-copilot/sessions"+path,{
      method:"POST",headers,credentials:"same-origin",redirect:"error",cache:"no-store",
      body:JSON.stringify({...payload,workspace_mode:getMode()}),keepalive
    });
    if(!r.ok)throw Error("Phiên GIS không khả dụng ("+r.status+")");
    return r.json();
  }
  async function nativeReq(s,action,payload,signal){
    if(requestAgent)return requestAgent(s,action,payload,signal);
    const r=await fetch("/ai-copilot/agent/"+s.session_id+"/"+action,{method:"POST",headers:{"X-OVG-AI":"1","X-OVG-Bridge":s.browser_token,"Content-Type":"application/json"},credentials:"same-origin",redirect:"error",cache:"no-store",body:JSON.stringify(payload),signal});
    if(!r.ok)throw Error("AI Agent không khả dụng ("+r.status+")");
    return r.json();
  }
  async function loadProviders(s){
    try{
      const data=await nativeReq(s,"providers",{});if(active!==s||disposed)return;
      if(data.platform?.version!==1)throw Error("Phiên bản AI không tương thích");
      providerChoices=Array.isArray(data.providers)?data.providers.filter(p=>typeof p.id==="string"&&typeof p.model==="string"):[];
      get("provider").replaceChildren(...providerChoices.map(p=>{const o=document.createElement("option");o.value=p.id;o.textContent=p.id+" · "+p.model;return o}));
      acpChoices=Array.isArray(data.acp_agents)?data.acp_agents.filter(x=>typeof x==="string"):[];
      get("acp-agent").replaceChildren(...acpChoices.map(x=>{const o=document.createElement("option");o.value=x;o.textContent=x;return o}));
      show("native",providerChoices.length>0||acpChoices.length>0);show("acp",acpChoices.length>0);get("run").disabled=!providerChoices.length;refreshVision();
    }catch{if(active===s)show("native",false)}
  }
  function clearImage(){
    image=null;get("preview").removeAttribute("src");show("preview",false);show("remove-image",false);get("vision-consent").checked=false;get("image-status").textContent="";
  }
  function refreshVision(){
    const p=providerChoices.find(p=>p.id===get("provider").value);
    show("vision",!!active&&p?.vision===true&&getMode()==="gis"&&typeof getBridge()?.snapshot==="function");
  }
  async function captureImage(){
    if(!active||disposed||nativeRun||capturing)return;
    const s=active,p=providerChoices.find(p=>p.id===get("provider").value),bridge=getBridge(),pin=context();
    if(p?.vision!==true||pin.mode!=="gis"||typeof bridge?.snapshot!=="function")return;
    capturing=true;get("capture").disabled=true;clearImage();
    try{
      const view=JSON.stringify(bridge.view());
      const shot=await bridge.snapshot();
      if(active!==s||disposed||p.id!==get("provider").value||JSON.stringify(context())!==JSON.stringify(pin)||JSON.stringify(bridge.view())!==view)throw Error("Bản đồ thay đổi, cần chụp lại.");
      image={shot,view,pin,at:performance.now(),provider:p.id};
      get("preview").src="data:image/jpeg;base64,"+shot.data;show("preview",true);show("remove-image",true);get("image-status").textContent=shot.width+" × "+shot.height+" px · ảnh tĩnh, chỉ canvas GIS";
    }catch{clearImage();get("image-status").textContent="Không thể chụp bản đồ hiện tại. Thử lại khi bản đồ đã tải xong."}
    finally{capturing=false;get("capture").disabled=false;}
  }
  async function runAcp(){
    if(!active||disposed||nativeRun||command||submitting||capturing)return;
    const s=active,agent=get("acp-agent").value,task=get("task").value.trim();
    if(!acpChoices.includes(agent)||!task||!get("acp-consent").checked){get("answer").textContent="Nhập yêu cầu và xác nhận gửi văn bản đến Coding Agent.";return}
    const controller=new AbortController();nativeRun=controller;get("acp-consent").checked=false;get("acp-run").disabled=true;get("run").disabled=true;show("cancel",true);get("answer").textContent="Coding Agent đang phân tích; chỉ trả về đề xuất.";
    try{
      const start=await nativeReq(s,"acp",{agent,task,consent:true},controller.signal);
      if(active!==s||disposed||controller.signal.aborted)return;
      if(typeof start.run_id!=="string"||start.state!=="running")throw Error("ACP job không hợp lệ");
      nativeJob={session:s,id:start.run_id};
      for(let i=0;i<75;i++){
        await new Promise(resolve=>setTimeout(resolve,2000));
        if(active!==s||disposed||controller.signal.aborted)return;
        const job=await nativeReq(s,"status",{run_id:start.run_id},controller.signal);
        if(job.state==="done"&&job.report){get("answer").textContent="Đề xuất ACP chưa được kiểm chứng · "+job.report.stop+"\n"+(job.report.answer||"");return}
        if(job.state!=="running")throw Error("ACP job không hợp lệ");
      }
      throw Error("Hết hạn chờ ACP");
    }catch{if(active===s&&!disposed){get("answer").textContent="Không xác minh được kết quả ACP. Không tự chạy lại.";void close();}}
    finally{if(nativeRun===controller){nativeRun=null;nativeJob=null;get("acp-run").disabled=false;get("run").disabled=!providerChoices.length;show("cancel",false)}}
  }
  async function runNative(){
    if(!active||disposed||nativeRun||command||submitting||capturing)return;
    const s=active,p=providerChoices.find(p=>p.id===get("provider").value),user=get("task").value.trim();
    if(!p||!user||!get("provider-consent").checked){get("answer").textContent="Chọn nhà cung cấp, nhập yêu cầu và xác nhận chia sẻ dữ liệu.";return}
    let snapshot=null;
    if(image){
      try{
        if(p.vision!==true||p.id!==image.provider||performance.now()-image.at>60000||JSON.stringify(context())!==JSON.stringify(image.pin)||JSON.stringify(getBridge().view())!==image.view)throw Error("stale");
        if(!get("vision-consent").checked){get("answer").textContent="Xác nhận riêng ảnh bản đồ trước khi gửi.";return}
        snapshot=image.shot;
      }catch{clearImage();get("answer").textContent="Ảnh không còn đúng vùng bản đồ. Chụp lại trước khi gửi.";return}
    }
    const controller=new AbortController();nativeRun=controller;
    get("run").disabled=true;get("provider").disabled=true;get("skill").disabled=true;get("task").disabled=true;get("writes").disabled=true;get("multi-agent").disabled=true;get("capture").disabled=true;get("remove-image").disabled=true;
    show("cancel",true);get("provider-consent").checked=false;get("answer").textContent="Agent đang xử lý. Duyệt từng lệnh GIS khi được yêu cầu.";
    try{
      const started=await nativeReq(s,"run",{provider:p.id,expected_model:p.model,user,skill:get("skill").value||null,consent:true,allow_writes:get("writes").checked,multi_agent:get("multi-agent").checked,...(snapshot?{snapshot,vision_consent:true}:{})},controller.signal);
      clearImage();
      if(active!==s||disposed||controller.signal.aborted)return;
      if(typeof started.run_id!=="string"||started.state!=="running")throw Error("Agent job không hợp lệ");
      nativeJob={session:s,id:started.run_id};let out=null;
      for(let i=0;i<160;i++){
        await new Promise(resolve=>setTimeout(resolve,2000));
        if(active!==s||disposed||controller.signal.aborted)return;
        const job=await nativeReq(s,"status",{run_id:started.run_id},controller.signal);
        if(job.state==="done"&&job.report){out=job.report;break}
        if(job.state!=="running")throw Error("Agent job không hợp lệ");
      }
      if(!out)throw Error("Hết hạn chờ Agent");
      if(active!==s||disposed)return;
      get("answer").textContent=(out.answer||("Trạng thái: "+out.stop))+"\nLệnh: "+out.calls+" · Vòng: "+out.model_steps+(out.uncertain_call?"\nCó lệnh chưa xác minh; không gửi lại.":"")+(out.unresolved_ticket?"\nTicket: "+out.unresolved_ticket:"")+(out.context?.compacted_groups?"\nNgữ cảnh: "+out.context.original_bytes+" → "+out.context.sent_bytes+" byte; "+out.context.compacted_groups+" nhóm đã nén.":"")+(Array.isArray(out.children)?out.children.map(c=>"\n"+c.role+" · "+c.stop+(c.report?": "+c.report:"")+(c.uncertain_call?" (chưa xác minh)":"")).join(""):"");
    }catch{
      if(active===s&&!disposed){get("answer").textContent="Mất phản hồi; không chạy lại. Đóng phiên và kiểm tra lịch sử lệnh.";void close();}
    }finally{
      if(nativeRun===controller){nativeRun=null;nativeJob=null;get("run").disabled=!providerChoices.length;get("provider").disabled=false;get("skill").disabled=false;get("task").disabled=false;get("writes").disabled=false;get("multi-agent").disabled=false;get("capture").disabled=false;get("remove-image").disabled=false;show("cancel",false);}
    }
  }
  function reset(message){
    if(nativeRun)nativeRun.abort();if(nativeJob){const job=nativeJob;nativeJob=null;void nativeReq(job.session,"cancel",{run_id:job.id}).catch(()=>{})}providerChoices=[];acpChoices=[];show("acp",false);get("acp-consent").checked=false;clearImage();show("vision",false);show("native",false);get("provider-consent").checked=false;get("writes").checked=false;get("multi-agent").checked=false;
    ++epoch;active=null;seen.clear();setAutomatic(false);setReview(null);show("connect",true);show("close",false);show("consent",false);
    get("connect").disabled=connecting;status(message);updateScope();
  }
  async function close(){
    const s=active;if(command)note(command,submitting?"Đã ngắt khi đang chạy; kiểm tra lịch sử máy chủ":"Đã ngắt trước khi duyệt");reset("Đã ngắt kết nối");
    if(s)try{await req("/"+s.session_id+"/close",s,{},true)}catch{}
  }
  async function start(){
    if(disposed||active||connecting||submitting)return;
    connecting=true;const startEpoch=epoch;get("connect").disabled=true;
    try{
      if(!getBridge())throw Error("Bản đồ chưa sẵn sàng");
      const s=await req("",null,{organization_id:organizationId,project_id:projectId,...(typeof getBridge()?.workspace==="function"?{workspace_version:2}:{})});
      if(!validSession(s))throw Error("Phản hồi phiên không hợp lệ");
      if(disposed||epoch!==startEpoch){void req("/"+s.session_id+"/close",s,{},true).catch(()=>{});return}
      active={...s,deadline:performance.now()+s.expires_in*1000};seen.clear();lastContextEpoch=getContextEpoch();++epoch;setAutomatic(false);show("connect",false);show("close",true);show("consent",true);
      status("Đã kết nối phiên này · tối đa 15 phút");updateScope();void loadProviders(active);
    }catch(e){if(!disposed)reset(e.message)}finally{connecting=false;if(!disposed)get("connect").disabled=!!active||submitting}
  }
  async function tick(){
    if(disposed)return;updateScope();if(getContextEpoch()!==lastContextEpoch){lastContextEpoch=getContextEpoch();setAutomatic(false);clearImage();refreshVision();if(nativeRun){void close();return}}
    if(!active||polling)return;
    if(performance.now()>=active.deadline){if(command)note(command,submitting?"Hết hạn khi đang chạy; cần đối chiếu":"Hết hạn");reset("Phiên đã hết hạn");return}
    const s=active,e=epoch,pollScope=context();
    polling=true;
    try{
      const path="/"+s.session_id+(command||submitting?"/ping":"/poll");
      const data=await req(path,s,{});
      if(active!==s||e!==epoch||disposed)return;
      if(command&&!submitting){try{assertCommandCurrent(command,context(),performance.now())}catch{note(command,"Hết hạn hoặc đổi workspace");void respond(false);return}}
      if(data.command&&!command&&!submitting){
        const c=data.command;
        const basic=["map_view","map_selection","map_navigate"].includes(c.method);
        const full=["gis_layers","gis_features","gis_feature","gis_query","gis_geometry_preview","gis_topology","gis_overlay","gis_layer_visibility","gis_select","gis_changeset","gis_revert","gis_create_layer","gis_schema","gis_export","gis_raster_layers","gis_raster_process","gis_raster_report","gis_raster_download","gis_processing_run","gis_processing_job","gis_models","gis_model_save","gis_model_run","gis_layouts","gis_layout_save","gis_layout_export","gis_plugins","gis_plugin_activate","gis_plugin_disable","gis_plugin_records","gis_plugin_create","gis_plugin_update"].includes(c.method)&&typeof getBridge()?.workspace==="function";
        const cad=["cad_state","cad_select_handle"].includes(c.method)&&typeof runCad==="function";
        if(seen.has(c.id)||seen.size>=256){reset("Nhận lại mã lệnh hoặc vượt giới hạn phiên; không chạy lại.");void req("/"+s.session_id+"/close",s,{},true).catch(()=>{});return}
        if((basic||full||cad)&&modeAllows(pollScope.mode,c.method)&&c.expires_in>2){
          const next=pinCommand(c,pollScope,performance.now());seen.add(next.id);note(next,"Chờ duyệt");
          try{assertCommandCurrent(next,context(),performance.now())}catch{setReview(next);void respond(false);return}
          if(autoAllowed&&SESSION_SAFE.has(c.method)){
            command=next;
            void respond(true,true);
          }else setReview(next);
        }else{reset("Lệnh không hỗ trợ hoặc hết hạn; đã đóng phiên để tránh chạy nhầm.");void req("/"+s.session_id+"/close",s,{},true).catch(()=>{});}
      }
    }catch{if(active===s){if(command)note(command,submitting?"Mất kết nối khi đang chạy; cần đối chiếu":"Mất kết nối trước khi duyệt");reset("Phiên hết hạn hoặc quyền GIS đã thay đổi");void req("/"+s.session_id+"/close",s,{},true).catch(()=>{})}}
    finally{polling=false}
  }
  async function respond(approve,automated=false){
    if(!active||!command||submitting||disposed)return;
    const s=active,c=command,e=epoch;
    if(automated&&(!autoAllowed||!SESSION_SAFE.has(c.method)))return;
    submitting=true;get("accept").disabled=true;get("deny").disabled=true;get("always").disabled=true;
    let ok=false,result=null,dispatched=false,reason="Đã từ chối";
    const guard=(minSeconds=0)=>{if(disposed||active!==s||e!==epoch||(automated&&!autoAllowed))throw Error("Phiên lệnh không còn hiệu lực");assertCommandCurrent(c,context(),performance.now(),minSeconds)};
    try{
      if(approve){
        // Fresh server authorization immediately before using the host map bridge.
        guard();note(c,"Đang xác minh quyền");
        const permission=await req("/"+s.session_id+"/commands/"+c.id+"/prepare",s,{});
        if(permission.authorized!==true||permission.expires_in<1)throw Error("Lệnh đã hết hạn");
        c.deadline=Math.min(c.deadline,performance.now()+permission.expires_in*1000);guard();
        const bridge=getBridge();dispatched=true;note(c,"Đang thực hiện");
        if(c.method.startsWith("cad_")){if(getMode()!=="cad"||typeof runCad!=="function")throw Error("Không ở chế độ CAD");result=await runCad(c.method,c.params??{});}
        else if(!bridge||getMode()==="cad"||(c.method.startsWith("map_")&&getMode()!=="gis"))throw Error("Lệnh không phù hợp chế độ đang mở");
        else if(c.method==="map_view")result=bridge.view();
        else if(c.method==="map_selection")result=bridge.selection();
        else if(c.method==="map_navigate"){
          const {longitude,latitude,zoom}=c.params??{};
          if(![longitude,latitude,zoom].every(Number.isFinite)||Math.abs(longitude)>180||Math.abs(latitude)>85||zoom<0||zoom>22)throw Error("Tọa độ không hợp lệ");
          bridge.navigate({longitude,latitude,zoom});result=null;
        }else if(c.method.startsWith("gis_")&&typeof bridge.workspace==="function"){
          if(permission.expires_in<15)throw Error("Không đủ thời gian thực hiện lệnh; hãy yêu cầu lại");
          result=await bridge.workspace(c.method,c.params??{},guard);
        }else throw Error("Lệnh không hỗ trợ");
        guard();ok=true;reason="Đã thực hiện";
      }
    }catch{result=null;reason=dispatched?"Không xác minh được kết quả; kiểm tra dữ liệu trước khi gửi lại":"Đã chặn trước khi thực thi";}
    try{
      if(active!==s||disposed||e!==epoch)return;
      const receipt=await req("/"+s.session_id+"/commands/"+c.id+"/ack",s,{ok,result});if(receipt.accepted!==true)throw Error("Biên nhận không hợp lệ");
      if(active===s){note(c,reason);status(ok?(automated?"Đã tự động thực hiện lệnh AI":"Đã thực hiện lệnh AI"):reason)}
    }catch{if(active===s){note(c,"Không xác nhận được biên nhận; không chạy lại");reset("Không nhận được xác nhận của bridge. Kiểm tra lịch sử trước khi yêu cầu lại.");void req("/"+s.session_id+"/close",s,{},true).catch(()=>{})}}
    finally{
      if(active===s)setReview(null);submitting=false;
      get("accept").disabled=false;get("deny").disabled=false;get("always").disabled=false;if(!disposed&&!active)get("connect").disabled=connecting;
    }
  }
  get("toggle").onclick=()=>{get("panel").hidden=!get("panel").hidden;};
  get("dismiss").onclick=()=>show("panel",false);
  get("auto").onchange=()=>{setAutomatic(get("auto").checked);status(autoAllowed?"Đã bật tự động duyệt lệnh an toàn":"Đã tắt tự động duyệt");};
  get("always").onclick=()=>{if(!command||!SESSION_SAFE.has(command.method))return;setAutomatic(true);void respond(true);};
  get("connect").onclick=start;get("close").onclick=close;
  get("run").onclick=runNative;get("acp-run").onclick=runAcp;get("acp-agent").onchange=()=>{get("acp-consent").checked=false};get("cancel").onclick=()=>{if(nativeRun)nativeRun.abort();void close()};get("provider").onchange=()=>{get("provider-consent").checked=false;clearImage();refreshVision()};get("capture").onclick=captureImage;get("remove-image").onclick=clearImage;
  get("accept").onclick=()=>respond(true);get("deny").onclick=()=>respond(false);
  const timer=setInterval(tick,1600);
  return ()=>{disposed=true;clearInterval(timer);void close();node.remove()};
}
