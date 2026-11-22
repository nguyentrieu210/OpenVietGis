/** First-party panel: mount inside a trusted, authenticated GIS workspace. */
export function mountAiCopilot(host, {organizationId, projectId, getBridge}) {
  const node=document.createElement("section");
  node.className="ovg-ai-copilot";
  node.innerHTML='<button type="button" data-ai="toggle">AI Copilot</button><div data-ai="panel" hidden><p data-ai="status" role="status">Chưa kết nối</p><button type="button" data-ai="connect">Kết nối AI</button><button type="button" data-ai="close" hidden>Ngắt kết nối</button><div data-ai="review" hidden role="region" aria-label="Xác nhận lệnh AI"><p data-ai="command"></p><pre data-ai="details" hidden></pre><button type="button" data-ai="accept">Đồng ý một lần</button><button type="button" data-ai="deny">Từ chối</button></div><p>Thêm kết nối MCP <code data-ai="endpoint"></code> vào ứng dụng AI và đăng nhập cùng tài khoản GIS.</p></div>';
  host.append(node);
  const get=name=>node.querySelector('[data-ai="'+name+'"]');
  get("endpoint").textContent=location.origin+"/mcp";
  let active=null,command=null,epoch=0,disposed=false,polling=false,submitting=false;
  const status=t=>{get("status").textContent=t;};
  const show=(k,flag)=>{get(k).hidden=!flag;};
  const setReview=(cmd)=>{
    command=cmd;show("review",!!cmd);
    if(cmd){
      const names={map_view:"Chia sẻ vùng nhìn bản đồ",map_selection:"Chia sẻ hình học đang chọn",map_navigate:"Di chuyển và zoom bản đồ",gis_layers:"Đọc danh sách lớp",gis_features:"Chia sẻ đối tượng và thuộc tính",gis_feature:"Đọc một đối tượng",gis_query:"Truy vấn thuộc tính / không gian",gis_geometry_preview:"Tính hình học CAD/phân tích",gis_topology:"Kiểm tra topology",gis_overlay:"Vẽ lớp xem trước",gis_layer_visibility:"Hiện/ẩn lớp",gis_select:"Chọn đối tượng",gis_changeset:"GHI DỮ LIỆU: tạo/sửa/xóa đối tượng",gis_revert:"GHI DỮ LIỆU: hoàn tác changeset",gis_create_layer:"GHI DỮ LIỆU: tạo lớp",gis_schema:"GHI DỮ LIỆU: thay đổi cấu trúc trường",gis_export:"Xuất GeoJSON"};
      get("command").textContent=names[cmd.method]||"Lệnh không hỗ trợ";
      const details=get("details");details.textContent=JSON.stringify(cmd.params??{},null,2);details.hidden=!Object.keys(cmd.params??{}).length;
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
    ++epoch;active=null;setReview(null);show("connect",true);show("close",false);
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
      const s=await req("",null,{organization_id:organizationId,project_id:projectId,...(typeof getBridge()?.workspace==="function"?{workspace_version:1}:{})});
      if(disposed){void req("/"+s.session_id+"/close",s,{},true).catch(()=>{});return}
      active=s;++epoch;show("connect",false);show("close",true);
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
        const full=["gis_layers","gis_features","gis_feature","gis_query","gis_geometry_preview","gis_topology","gis_overlay","gis_layer_visibility","gis_select","gis_changeset","gis_revert","gis_create_layer","gis_schema","gis_export"].includes(c.method)&&typeof getBridge()?.workspace==="function";
        if((basic||full)&&c.expires_in>2){
          setReview({...c,deadline:performance.now()+c.expires_in*1000});
        }
      }
    }catch{if(active===s)reset("Phiên hết hạn hoặc quyền GIS đã thay đổi")}
    finally{polling=false}
  }
  async function respond(approve){
    if(!active||!command||submitting||disposed)return;
    const s=active,c=command,e=epoch;
    submitting=true;get("accept").disabled=true;get("deny").disabled=true;
    let ok=false,result=null;
    try{
      if(approve){
        // Fresh server authorization immediately before using the host map bridge.
        const permission=await req("/"+s.session_id+"/commands/"+c.id+"/prepare",s,{});
        if(permission.authorized!==true||permission.expires_in<1)throw Error("Lệnh đã hết hạn");
        if(disposed||active!==s||e!==epoch||performance.now()>=c.deadline)throw Error("Lệnh đã hết hạn");
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
          result=await bridge.workspace(c.method,c.params??{},()=>{if(disposed||active!==s||e!==epoch||performance.now()>=c.deadline)throw Error("Phiên lệnh AI đã hết hạn hoặc bị ngắt");});
        }else throw Error("Lệnh không hỗ trợ");
        ok=true;
      }
    }catch{}
    try{
      if(active!==s||disposed||e!==epoch)return;
      await req("/"+s.session_id+"/commands/"+c.id+"/ack",s,{ok,result});
      if(active===s)status(ok?"Đã thực hiện lệnh AI":"Đã từ chối hoặc không thể thực hiện");
    }catch{if(active===s)status("Không nhận được xác nhận của bridge")}
    finally{
      if(active===s)setReview(null);submitting=false;
      get("accept").disabled=false;get("deny").disabled=false;
    }
  }
  get("toggle").onclick=()=>{get("panel").hidden=!get("panel").hidden;};
  get("connect").onclick=start;get("close").onclick=close;
  get("accept").onclick=()=>respond(true);get("deny").onclick=()=>respond(false);
  const timer=setInterval(tick,1600);
  return ()=>{disposed=true;clearInterval(timer);void close();node.remove()};
}
