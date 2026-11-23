/** First-party panel: mount inside a trusted, authenticated GIS workspace. */
export function mountAiCopilot(host, {organizationId, projectId, getBridge}) {
  // Explicit session consent is limited to these read-only / temporary commands.
  // All downloads, persisted writes, administration and unknown tools are reviewed individually.
  const SESSION_SAFE=new Set(["map_view","map_selection","map_navigate","gis_layers","gis_features","gis_feature","gis_query","gis_geometry_preview","gis_topology","gis_overlay","gis_layer_visibility","gis_select","gis_raster_layers","gis_processing_job","gis_models","gis_layouts","gis_plugins","gis_plugin_records"]);
  const node=document.createElement("section");
  node.className="ovg-ai-copilot";
  node.innerHTML='<button type="button" data-ai="toggle"><span aria-hidden="true">✦</span> AI Copilot</button><div data-ai="panel" hidden><header data-ai="header"><div><span data-ai="eyebrow">OPENVIETGIS · AI WORKSPACE</span><h2>Trợ lý bản đồ AI</h2><p>Điều khiển và phân tích GIS bằng ngôn ngữ tự nhiên.</p></div><button type="button" data-ai="dismiss" aria-label="Thu gọn AI Copilot">×</button></header><div data-ai="connection"><p data-ai="status" role="status">Chưa kết nối</p><button type="button" data-ai="connect">Kết nối AI</button><button type="button" data-ai="close" hidden>Ngắt kết nối</button></div><div data-ai="consent" hidden><label><input type="checkbox" data-ai="auto"> <strong>Luôn cho phép lệnh an toàn trong phiên</strong></label><p>Đọc dữ liệu, phân tích xem trước, di chuyển bản đồ và chọn đối tượng. Có thể tắt bất cứ lúc nào. Tạo, sửa, xóa, xuất dữ liệu và quản trị luôn hỏi riêng.</p></div><div data-ai="review" hidden role="region" aria-label="Xác nhận lệnh AI"><span data-ai="risk"></span><h3 data-ai="command"></h3><p data-ai="explain"></p><details data-ai="detailbox" hidden><summary>Thông số và đối tượng liên quan</summary><pre data-ai="details"></pre></details><div data-ai="actions"><button type="button" data-ai="always" hidden>Luôn cho phép trong phiên</button><button type="button" data-ai="accept">Cho phép lần này</button><button type="button" data-ai="deny">Từ chối</button></div></div><footer data-ai="footer"><strong>Kết nối ChatGPT / Claude</strong><p>Đăng nhập cùng tài khoản GIS, chọn dự án rồi kết nối. Phiên có hiệu lực tối đa 15 phút.</p><code data-ai="endpoint"></code></footer></div>';
  host.append(node);
  const get=name=>node.querySelector('[data-ai="'+name+'"]');
  get("endpoint").textContent=location.origin+"/mcp";
  let active=null,command=null,epoch=0,disposed=false,polling=false,submitting=false,autoAllowed=false;
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
      const names={map_view:"Chia sẻ vùng nhìn bản đồ",map_selection:"Chia sẻ hình học đang chọn",map_navigate:"Di chuyển và zoom bản đồ",gis_layers:"Đọc danh sách lớp",gis_features:"Chia sẻ đối tượng và thuộc tính",gis_feature:"Đọc một đối tượng",gis_query:"Truy vấn thuộc tính / không gian",gis_geometry_preview:"Tính hình học CAD/phân tích",gis_topology:"Kiểm tra topology",gis_overlay:"Vẽ lớp xem trước",gis_layer_visibility:"Hiện/ẩn lớp",gis_select:"Chọn đối tượng",gis_changeset:"GHI DỮ LIỆU: tạo/sửa/xóa đối tượng",gis_revert:"GHI DỮ LIỆU: hoàn tác changeset",gis_create_layer:"GHI DỮ LIỆU: tạo lớp",gis_schema:"GHI DỮ LIỆU: thay đổi cấu trúc trường",gis_export:"Xuất GeoJSON",gis_raster_layers:"Đọc lớp raster",gis_raster_process:"GHI DỮ LIỆU: xử lý raster",gis_raster_report:"Báo cáo raster",gis_raster_download:"Tải GeoTIFF",gis_processing_run:"GHI DỮ LIỆU: chạy xử lý vector",gis_processing_job:"Đọc trạng thái processing",gis_models:"Đọc mô hình xử lý",gis_model_save:"GHI DỮ LIỆU: lưu mô hình",gis_model_run:"GHI DỮ LIỆU: chạy toàn bộ mô hình",gis_layouts:"Đọc layout",gis_layout_save:"GHI DỮ LIỆU: lưu layout",gis_layout_export:"Xuất layout PDF/PNG/JSON",gis_plugins:"Đọc danh mục plugin",gis_plugin_activate:"QUẢN TRỊ: bật plugin",gis_plugin_disable:"QUẢN TRỊ: tắt plugin",gis_plugin_records:"Đọc bản ghi plugin",gis_plugin_create:"GHI DỮ LIỆU: tạo bản ghi plugin",gis_plugin_update:"GHI DỮ LIỆU: sửa bản ghi plugin"};
      get("command").textContent=names[cmd.method]||"Lệnh không hỗ trợ";
      const safe=SESSION_SAFE.has(cmd.method);
      get("risk").textContent=safe?"LỆNH AN TOÀN · KHÔNG GHI DỮ LIỆU":"CẦN XÁC NHẬN RIÊNG";
      get("risk").dataset.safe=String(safe);
      get("explain").textContent=safe?"AI yêu cầu thao tác đọc hoặc xem trước trong dự án đang mở. Không thay đổi dữ liệu lưu trữ.":"Thao tác có thể thay đổi dữ liệu, tải tệp hoặc quản trị dự án. Kiểm tra thông số trước khi cho phép.";
      show("always",safe);
      const details=get("details");details.textContent=JSON.stringify(cmd.params??{},null,2);
      show("detailbox",Object.keys(cmd.params??{}).length>0);
      get("detailbox").open=!safe;
    }
  };
  async function req(path,session,payload,keepalive=false){
    const headers={"X-OVG-AI":"1","Content-Type":"application/json"};
    if(session)headers["X-OVG-Bridge"]=session.browser_token;
    const r=await fetch("/ai-copilot/sessions"+path,{
      method:"POST",headers,credentials:"same-origin",redirect:"error",cache:"no-store",
      body:JSON.stringify(payload??{}),keepalive
    });
    if(!r.ok)throw Error("Phiên GIS không khả dụng ("+r.status+")");
    return r.json();
  }
  function reset(message){
    ++epoch;active=null;setAutomatic(false);setReview(null);show("connect",true);show("close",false);show("consent",false);
    get("connect").disabled=false;status(message);
  }
  async function close(){
    const s=active;reset("Đã ngắt kết nối");
    if(s)try{await req("/"+s.session_id+"/close",s,{},true)}catch{}
  }
  async function start(){
    if(disposed||active)return;
    get("connect").disabled=true;
    try{
      if(!getBridge())throw Error("Bản đồ chưa sẵn sàng");
      const s=await req("",null,{organization_id:organizationId,project_id:projectId,...(typeof getBridge()?.workspace==="function"?{workspace_version:2}:{})});
      if(disposed){void req("/"+s.session_id+"/close",s,{},true).catch(()=>{});return}
      active=s;++epoch;setAutomatic(false);show("connect",false);show("close",true);show("consent",true);
      status("Đã kết nối phiên này · 15 phút");
    }catch(e){if(!disposed)reset(e.message)}
  }
  async function tick(){
    if(!active||disposed||polling)return;
    const s=active,e=epoch;
    polling=true;
    try{
      const path="/"+s.session_id+(command||submitting?"/ping":"/poll");
      const data=await req(path,s,{});
      if(active!==s||e!==epoch||disposed)return;
      if(command&&performance.now()>=command.deadline)setReview(null);
      if(data.command&&!command&&!submitting){
        const c=data.command;
        const basic=["map_view","map_selection","map_navigate"].includes(c.method);
        const full=["gis_layers","gis_features","gis_feature","gis_query","gis_geometry_preview","gis_topology","gis_overlay","gis_layer_visibility","gis_select","gis_changeset","gis_revert","gis_create_layer","gis_schema","gis_export","gis_raster_layers","gis_raster_process","gis_raster_report","gis_raster_download","gis_processing_run","gis_processing_job","gis_models","gis_model_save","gis_model_run","gis_layouts","gis_layout_save","gis_layout_export","gis_plugins","gis_plugin_activate","gis_plugin_disable","gis_plugin_records","gis_plugin_create","gis_plugin_update"].includes(c.method)&&typeof getBridge()?.workspace==="function";
        if((basic||full)&&c.expires_in>2){
          const next={...c,deadline:performance.now()+c.expires_in*1000};
          if(autoAllowed&&SESSION_SAFE.has(c.method)){
            command=next;
            void respond(true,true);
          }else setReview(next);
        }
      }
    }catch{if(active===s)reset("Phiên hết hạn hoặc quyền GIS đã thay đổi")}
    finally{polling=false}
  }
  async function respond(approve,automated=false){
    if(!active||!command||submitting||disposed)return;
    const s=active,c=command,e=epoch;
    if(automated&&(!autoAllowed||!SESSION_SAFE.has(c.method)))return;
    submitting=true;get("accept").disabled=true;get("deny").disabled=true;get("always").disabled=true;
    let ok=false,result=null;
    try{
      if(approve){
        // Fresh server authorization immediately before using the host map bridge.
        const permission=await req("/"+s.session_id+"/commands/"+c.id+"/prepare",s,{});
        if(permission.authorized!==true||permission.expires_in<1)throw Error("Lệnh đã hết hạn");
        if(disposed||active!==s||e!==epoch||performance.now()>=c.deadline||(automated&&!autoAllowed))throw Error("Lệnh đã hết hạn hoặc quyền tự động đã bị thu hồi");
        const bridge=getBridge();
        if(!bridge)throw Error("Bản đồ không sẵn sàng");
        if(c.method==="map_view")result=bridge.view();
        else if(c.method==="map_selection")result=bridge.selection();
        else if(c.method==="map_navigate"){
          const {longitude,latitude,zoom}=c.params??{};
          if(![longitude,latitude,zoom].every(Number.isFinite)||Math.abs(longitude)>180||Math.abs(latitude)>85||zoom<0||zoom>22)throw Error("Tọa độ không hợp lệ");
          bridge.navigate({longitude,latitude,zoom});result=null;
        }else if(c.method.startsWith("gis_")&&typeof bridge.workspace==="function"){
          if(permission.expires_in<15)throw Error("Không đủ thời gian thực hiện lệnh; hãy yêu cầu lại");
          result=await bridge.workspace(c.method,c.params??{},(minSeconds=0)=>{if(disposed||active!==s||e!==epoch||performance.now()+minSeconds*1000>=c.deadline)throw Error("Phiên lệnh AI đã hết hạn, bị ngắt hoặc không đủ thời gian");});
        }else throw Error("Lệnh không hỗ trợ");
        ok=true;
      }
    }catch{}
    try{
      if(active!==s||disposed||e!==epoch)return;
      await req("/"+s.session_id+"/commands/"+c.id+"/ack",s,{ok,result});
      if(active===s)status(ok?(automated?"Đã tự động thực hiện lệnh AI":"Đã thực hiện lệnh AI"):"Đã từ chối hoặc không thể thực hiện");
    }catch{if(active===s)status("Không nhận được xác nhận của bridge")}
    finally{
      if(active===s)setReview(null);submitting=false;
      get("accept").disabled=false;get("deny").disabled=false;get("always").disabled=false;
    }
  }
  get("toggle").onclick=()=>{get("panel").hidden=!get("panel").hidden;};
  get("dismiss").onclick=()=>show("panel",false);
  get("auto").onchange=()=>{setAutomatic(get("auto").checked);status(autoAllowed?"Đã bật tự động duyệt lệnh an toàn":"Đã tắt tự động duyệt");};
  get("always").onclick=()=>{if(!command||!SESSION_SAFE.has(command.method))return;setAutomatic(true);void respond(true);};
  get("connect").onclick=start;get("close").onclick=close;
  get("accept").onclick=()=>respond(true);get("deny").onclick=()=>respond(false);
  const timer=setInterval(tick,1600);
  return ()=>{disposed=true;clearInterval(timer);void close();node.remove()};
}
