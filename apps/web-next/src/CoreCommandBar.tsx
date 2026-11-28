import "./ui-menu.css";
import React,{useEffect,useMemo,useRef,useState} from "react";
import {createPortal} from "react-dom";
import {normalizeCommand as normalize,searchCommands,resolveCommand,searchRibbonCommands} from "./command-search";
export type CoreCommand={alias:string;label:string;group:string;hint?:string;pluginMenu?:{id:string;name:string;label:string;checked?:boolean;section?:string};disabled?:boolean;disabledReason?:string;run:(args:string)=>void|Promise<void>};
const aliases:Record<string,string>={L:"LINE",PL:"POLYGON",PO:"POINT",M:"MOVE",CO:"COPY",CP:"COPY",O:"OFFSET",RO:"ROTATE",SC:"SCALE",S:"STRETCH",TR:"TRIM",EX:"EXTEND",F:"FILLET",E:"ERASE",DI:"DIST",AA:"AREA",P:"PAN",Z:"ZOOM",U:"UNDO",OS:"OSNAP",QSELECT:"SELRECT",RE:"RELOAD"};
type Host={commands:CoreCommand[];cancel:()=>string;activePrompt?:string;finishActive:()=>string|undefined;inputActive:(value:string)=>string|undefined;shortcut:(key:"F3"|"F8")=>string|undefined};
const editable=(t:HTMLElement|null)=>!!t&&(t.isContentEditable||["INPUT","TEXTAREA","SELECT"].includes(t.tagName)||!!t.closest('[contenteditable="true"]'));
export function CoreCommandBar(host:Host){
 const [value,setValue]=useState(""),[history,setHistory]=useState<string[]>([]),[log,setLog]=useState<string[]>([]),[status,setStatus]=useState("Sẵn sàng"),[focused,setFocused]=useState(false),[help,setHelp]=useState(false),[showHistory,setShowHistory]=useState(false),[cursor,setCursor]=useState(-1),[highlight,setHighlight]=useState(-1),[pending,setPending]=useState<"ZOOM"|"PAN"|null>(null);
 const ref=useRef<HTMLInputElement>(null),root=useRef<HTMLDivElement>(null),epoch=useRef(0),last=useRef(""),running=useRef(false);
 const query=value.trim();
 const matches=useMemo(()=>searchCommands(host.commands,help?"":query,aliases,help?host.commands.length:8),[host.commands,query,help]);
 const report=(line:string)=>{setStatus(line);setLog(l=>[...l,line].slice(-80))};
 const focus=()=>{if(!document.querySelector('[aria-modal="true"]'))ref.current?.focus({preventScroll:true});else requestAnimationFrame(()=>{if(!document.querySelector('[aria-modal="true"]'))ref.current?.focus({preventScroll:true})})};
 const clear=()=>{setValue("");setHelp(false);setHighlight(-1);setCursor(-1);setPending(null)};
 function escape(){epoch.current++;clear();setShowHistory(false);const result=host.cancel();report(result);focus()}
 async function execute(raw:string,fromMenu=false){
  if(running.current||!root.current||root.current.closest("[inert]"))return;
  const input=raw.trim();
  if(!input&&!pending){const result=host.finishActive();if(result){clear();report(result);return}if(!last.current){focus();return}return execute(last.current)}
  if(!input)return;
  if(["ESC","CANCEL"].includes(normalize(input))){escape();return}
  const active=fromMenu||pending?undefined:host.inputActive(input);if(active!==undefined){clear();report(active);focus();return}
  let text=input;
  if(pending&&!fromMenu){if(pending==="ZOOM"){const z=normalize(input);text=({E:"ZE",I:"ZI",O:"ZO",L:"ZL",S:"ZS"} as Record<string,string>)[z]??"ZOOM "+input}else text="PAN "+input}
  const [token,...rest]=text.split(/\s+/),key=normalize(token),alias=aliases[key]??key;
  if(alias==="ESC"||alias==="CANCEL"){escape();return}
  if(alias==="HELP"||alias==="?"){clear();setHelp(true);setFocused(true);report("Danh sách lệnh · bấm Tab để điền");focus();return}
  if(alias==="ZOOM"&&rest.length&&["E","I","O","L","S"].includes(normalize(rest[0]))){void execute("Z"+normalize(rest[0]));return}
  if((alias==="ZOOM"||alias==="PAN")&&!rest.length){setPending(alias);setValue("");setHelp(false);report(alias==="ZOOM"?"Zoom [E toàn bộ / I / O / L / S / mức zoom]":"Nhập kinh độ,vĩ độ");focus();return}
  const command=resolveCommand(host.commands,token,aliases);
  // Not a command: one box like the QGIS locator, so search features instead.
  if(!command){setPending(null);clear();report("Tìm: "+text);window.dispatchEvent(new CustomEvent("ovg:search",{detail:text}));return}
  if(command.disabled){report(command.alias+" · "+commandDisabledReason(command));setPending(null);return}
  const full=[command.alias,...rest].join(" ");if(!host.activePrompt||!["ZE","ZI","ZO","ZL","ZS","ZOOM","PAN","ORTHO","OSNAP","SNAP"].includes(command.alias))last.current=full;setHistory(h=>[full,...h.filter(x=>x!==full)].slice(0,50));setLog(l=>[...l,"Command: "+full].slice(-80));
  const seq=++epoch.current;clear();setFocused(false);running.current=true;
  try{await command.run(rest.join(" "));if(seq===epoch.current)report(command.alias+" · "+command.label)}catch(e){if(seq===epoch.current)report(command.alias+" · "+(e instanceof Error?e.message:"Không thể thực hiện"))}finally{running.current=false}
 }
 const latest=useRef({execute,escape,focus,clear,shortcut:host.shortcut,activePrompt:host.activePrompt});latest.current={execute,escape,focus,clear,shortcut:host.shortcut,activePrompt:host.activePrompt};
 useEffect(()=>{const key=(e:KeyboardEvent)=>{
  if(!root.current||root.current.closest("[inert]")||document.querySelector(".core-ribbon-menu-fixed"))return;
  if(e.defaultPrevented||e.isComposing||e.repeat&&["Escape","F2","F3","F8"].includes(e.key))return;
  const t=e.target as HTMLElement|null,own=t===ref.current,modal=!!document.querySelector('[aria-modal="true"]'),otherForm=editable(t)&&!own,button=!!t?.closest("button,a,[role=button]");
  if(e.key==="Escape"){if(modal||otherForm||document.querySelector(".qgis-context,.dock-menu-pop"))return;e.preventDefault();latest.current.escape();return}
  if(modal||e.altKey)return;
  if((e.ctrlKey||e.metaKey)&&!e.shiftKey&&e.key.toLowerCase()==="k"){e.preventDefault();e.stopPropagation();latest.current.focus();return}
  if(otherForm)return;
  if((e.ctrlKey||e.metaKey)&&(!own||!ref.current?.value)&&["z","y"].includes(e.key.toLowerCase())){e.preventDefault();e.stopPropagation();void latest.current.execute(e.key.toLowerCase()==="y"||e.shiftKey?"REDO":"UNDO");return}
  if(e.ctrlKey||e.metaKey)return;
  if(e.key==="F2"&&t?.closest(".qgis-tree"))return;
  if(e.key==="F2"){e.preventDefault();e.stopPropagation();setShowHistory(v=>!v);setHelp(false);return}
  if(e.key==="F3"||e.key==="F8"){e.preventDefault();e.stopPropagation();const result=latest.current.shortcut(e.key);if(result)report(result);return}
  if(own)return;
  if(e.key==="/"&&!button){e.preventDefault();latest.current.focus();return}
  // Enter: finish what is in progress, otherwise open the command line (plain keys are shortcuts).
  if(e.key==="Enter"&&!button&&!latest.current.activePrompt){e.preventDefault();e.stopPropagation();latest.current.focus();return}
  if(e.key==="Enter"&&!button){e.preventDefault();e.stopPropagation();void latest.current.execute("");return}
 };window.addEventListener("keydown",key,true);return()=>window.removeEventListener("keydown",key,true)},[]);
 useEffect(()=>{const run=(e:Event)=>{const alias=(e as CustomEvent<string>).detail;void latest.current.execute(alias,true)};window.addEventListener("ovg:menu-command",run);return()=>window.removeEventListener("ovg:menu-command",run)},[]);
 // Right-click no longer repeats the last command (AutoCAD habit): in QGIS it opens context menus / finishes digitizing.
 useEffect(()=>{if(highlight>=0)root.current?.querySelector('[data-highlight="true"]')?.scrollIntoView({block:"nearest"})},[highlight]);
 useEffect(()=>{const out=(e:PointerEvent)=>{if(root.current&&!root.current.contains(e.target as Node)){setFocused(false);setHelp(false);setShowHistory(false)}};document.addEventListener("pointerdown",out);return()=>document.removeEventListener("pointerdown",out)},[]);
 const open=focused&&(help||!!query&&!pending)&&matches.length>0;
 const placeholder=pending==="ZOOM"?"Zoom: E / I / O / L / S / mức zoom":pending==="PAN"?"Kinh độ,vĩ độ":host.activePrompt??(status==="Sẵn sàng"?"Ctrl+K · lệnh, layer: tên, xy: kinh độ,vĩ độ…":status);
 return <div className="core-command" ref={root}>
 <form onSubmit={e=>{e.preventDefault();void execute(value)}}><span className="core-command-prompt" aria-hidden="true">⌘</span>
 <input ref={ref} aria-label="Dòng lệnh GIS" autoComplete="off" spellCheck={false} placeholder={placeholder} aria-keyshortcuts="Control+K Meta+K" value={value} onFocus={()=>setFocused(true)} onBlur={()=>setFocused(false)} onChange={e=>{setValue(e.target.value);setFocused(true);setCursor(-1);setHighlight(-1);setHelp(false)}} onKeyDown={e=>{
  if(e.key==="Tab"&&open){e.preventDefault();const c=matches[Math.max(0,highlight)];if(c){setValue(c.alias);setHelp(false);setHighlight(-1)}}
  else if(e.key==="ArrowUp"||e.key==="ArrowDown"){e.preventDefault();if(open&&value.trim()&&cursor<0){setHighlight(i=>e.key==="ArrowDown"?(i+1)%matches.length:(i<=0?matches.length-1:i-1))}else{const i=e.key==="ArrowUp"?Math.min(history.length-1,cursor+1):Math.max(-1,cursor-1);setCursor(i);setValue(i<0?"":history[i]);setHelp(false);setHighlight(-1)}}
  else if(e.key==="Enter"&&open&&highlight>=0&&matches[highlight]){e.preventDefault();void execute(matches[highlight].alias,true)}

 }}/>
 <button type="submit" aria-label="Thực thi lệnh" title="Thực thi · Enter">↵</button><button type="button" aria-label="Lịch sử lệnh (F2)" title="Lịch sử lệnh · F2" aria-expanded={showHistory} onClick={()=>{setShowHistory(v=>!v);setHelp(false)}}>≡</button>
 <button type="button" aria-label="Danh sách lệnh" title="Danh sách lệnh · HELP" onClick={()=>{setValue("");setHelp(v=>!v);setFocused(true);setShowHistory(false);focus()}}>?</button>
 </form>
 <div className="core-command-status" hidden={status==="Sẵn sàng"&&!host.activePrompt}><small role="status">{status}</small></div>
 {showHistory&&<div className="core-command-history" aria-label="Lịch sử lệnh"><header><b>Lịch sử lệnh</b><button onClick={()=>setShowHistory(false)} aria-label="Đóng lịch sử lệnh">×</button></header>{log.length?log.map((l,i)=><div key={i}>{l}</div>):<p>Chưa có lệnh.</p>}</div>}
 {open&&!showHistory&&<div className="core-command-list" role="listbox" aria-label="Lệnh khả dụng">{matches.map((c,i)=><button type="button" key={c.alias} data-highlight={i===highlight} disabled={c.disabled} title={c.disabled?commandDisabledReason(c):c.hint} onMouseDown={e=>e.preventDefault()} onClick={()=>void execute(c.alias,true)}><code>{c.alias}</code><span>{c.label}<small>{c.group}{c.disabled?" · "+commandDisabledReason(c):c.hint?" · "+c.hint:""}</small></span></button>)}</div>}
 </div>
}
// Menus ordered like QGIS (Project, Edit, View, Layer, Settings, Plugins, Vector, Raster, …, Processing); unknown commands keep their own group.
const QGIS_MENUS:Record<string,string[]>={"Dự án":["SAVE","VIEW","HISTORY","RELOAD","LAYOUT","PRINTCLASSIC","PLOT"],"Biên tập":["UNDO","REDO","EDIT","POINT","LINE","POLYGON","MOVE","VERTEX","SPLIT","MERGE","ATTR_EDIT","ATTR_FORM","BULK","ERASE","DELSEL","EDITHISTORY","SNAP"],"Hiển thị":["ZI","ZO","ZE","ZL","ZS","ZOOM","PAN","SELRECT","SELPOLY","CLR","DESELECT","DIST","AREA","END","ESC","BASE","TILES","LAYERS"],"Lớp":["ADD","NEWLAYER","LA","WMS","LAYER","LABEL","STYLE","ATTR","UP","DOWN","HIDE","SHOW","IMPORT","EXPORT"],"Thiết lập":["CRS","WORKSPACE","OPS"],"Plugin":["PLUGIN","FIELD","REVIEW"],"Vector":["BUFFER","CLIP","DISSOLVE","DIFFERENCE","INTERSECTION","UNION","REPROJECT","SPATIALJOIN","QUERY","TOPOLOGY","SCHEMA","CALC","JOIN","STATS"],"Raster":["RASTER","RASTERCALC","RASTERCLIP","MOSAIC","RESAMPLE","RGB","NDVI","NDWI","NDBI","SENTINEL","CHANGE","ZONAL","SLOPE","ASPECT","HILLSHADE","CONTOUR","TERRAIN"],"CAD":["DXF","COGO","SURVEY","ORTHO","OSNAP"],"Xử lý":["TOOLBOX","MODELER","JOBS"]};
const QGIS_MENU_OF=new Map(Object.entries(QGIS_MENUS).flatMap(([menu,aliases])=>aliases.map(a=>[a,menu] as const)));
const QGIS_RANK=new Map(Object.values(QGIS_MENUS).flatMap(a=>a.map((x,i)=>[x,i] as const)));
const menuTopics:Record<string,string>={"Dự án":"Lưu, góc nhìn, lịch sử và in bản đồ","Biên tập":"Vẽ, sửa hình học, thuộc tính và hoàn tác","Hiển thị":"Phóng to, chọn, đo đạc và bản đồ nền","Lớp":"Thêm, tạo, ký hiệu, nhãn và bảng thuộc tính","Thiết lập":"Hệ tọa độ và tùy chỉnh","Plugin":"Plugin, thực địa và cộng tác","Vector":"Xử lý vector, trường dữ liệu và thống kê","Raster":"Phân tích ảnh, chỉ số và địa hình","CAD":"Công cụ CAD, DXF và trắc địa","Xử lý":"Hộp công cụ, Modeler và tác vụ","Khác":"Lệnh của plugin và công cụ đối tác"};
const needs:Record<string,string>={MOVE:"Chọn một điểm có quyền sửa hình học.",POINT:"Chọn layer điểm có quyền tạo đối tượng.",LINE:"Chọn layer đường có quyền tạo đối tượng.",POLYGON:"Chọn layer vùng có quyền tạo đối tượng.",ZL:"Chọn layer trong cây lớp.",ZS:"Chọn một đối tượng trên bản đồ.",LAYER:"Chọn layer trong cây lớp.",VERTEX:"Chọn đối tượng hình học có quyền sửa.",ERASE:"Chọn đối tượng có quyền xóa.",END:"Bắt đầu phép đo hoặc bản vẽ trước.",ESC:"Chỉ dùng khi có thao tác đang thực hiện.",LAYOUT:"Chờ bản đồ sẵn sàng.",DESELECT:"Chưa có đối tượng được chọn."};
export function commandDisabledReason(c:CoreCommand){
 return c.disabledReason??needs[c.alias]??"Cần chọn layer/đối tượng, quyền phù hợp hoặc hoàn tất công cụ đang mở.";
}
export function CoreRibbon({commands,enabled=true}:{commands:CoreCommand[];enabled?:boolean}){
 commands=commands.map(c=>({...c,group:QGIS_MENU_OF.get(c.alias)??(c.group==="Plugin"?"Plugin":"Khác")})).sort((a,b)=>(QGIS_RANK.get(a.alias)??99)-(QGIS_RANK.get(b.alias)??99));
 const [group,setGroup]=useState(""),[query,setQuery]=useState(""),[pos,setPos]=useState({x:0,y:0});
 const [sub,setSub]=useState<{id:string;x:number;y:number}|null>(null);const subTrigger=useRef<HTMLButtonElement|null>(null),subMenu=useRef<HTMLDivElement>(null);
 const groups=Object.keys(menuTopics),root=useRef<HTMLDivElement>(null),menu=useRef<HTMLDivElement>(null);
 const trigger=useRef<HTMLButtonElement|null>(null),search=useRef<HTMLInputElement>(null);
 const close=(restore=false)=>{setSub(null);setGroup("");setQuery("");if(restore)trigger.current?.focus({preventScroll:true})};
 useEffect(()=>{if(!enabled){setGroup("");setQuery("")}else if(group){const target=search.current?.getClientRects().length?search.current:menu.current?.querySelector<HTMLButtonElement>(".core-menu-items button:not(:disabled)")??menu.current;target?.focus({preventScroll:true})}},[group,enabled]);
 useEffect(()=>{const outside=(e:PointerEvent)=>{if(!root.current?.contains(e.target as Node)&&!menu.current?.contains(e.target as Node))close()};const key=(e:KeyboardEvent)=>{if(e.key==="Escape"&&menu.current){e.stopImmediatePropagation();e.preventDefault();if(subMenu.current){setSub(null);subTrigger.current?.focus()}else close(true)}};const resize=()=>close();document.addEventListener("pointerdown",outside);window.addEventListener("keydown",key,true);window.addEventListener("resize",resize);return()=>{document.removeEventListener("pointerdown",outside);window.removeEventListener("keydown",key,true);window.removeEventListener("resize",resize)}},[]);
 const menuKeys=(e:React.KeyboardEvent<HTMLDivElement>)=>{
  if(e.key==="ArrowLeft"&&subMenu.current){e.preventDefault();setSub(null);subTrigger.current?.focus();return}if(!["ArrowDown","ArrowUp","Home","End"].includes(e.key)||e.altKey||e.ctrlKey||e.metaKey)return;
  if((e.key==="Home"||e.key==="End")&&e.target===search.current)return;
  const options=Array.from((subMenu.current??menu.current)?.querySelectorAll<HTMLButtonElement>(subMenu.current?"button:not(:disabled)":".core-menu-items button:not(:disabled)")??[]);
  if(!options.length)return;const at=options.indexOf(document.activeElement as HTMLButtonElement);
  const next=e.key==="Home"?0:e.key==="End"?options.length-1:e.key==="ArrowDown"?(at+1)%options.length:(at<=0?options.length-1:at-1);
  e.preventDefault();options[next]?.focus();
 };
 const items=searchRibbonCommands(commands.filter(c=>group==="Tìm lệnh"||c.group===group),query,aliases);
 const open=(e:React.MouseEvent<HTMLButtonElement>,g:string)=>{if(!enabled||document.querySelector('[aria-modal="true"]'))return;if(group===g){close(true);return}trigger.current=e.currentTarget;const r=e.currentTarget.getBoundingClientRect();setPos({x:Math.max(8,Math.min(r.left,innerWidth-Math.min(g==="Tìm lệnh"?420:340,innerWidth-16)-8)),y:r.bottom+5});setSub(null);setQuery("");setGroup(g)};
 const pluginGroups=new Map<string,{name:string;commands:CoreCommand[]}>();
 if(group==="Plugin")for(const c of items){if(!c.pluginMenu)continue;const p=c.pluginMenu,g=pluginGroups.get(p.id)??{name:p.name,commands:[]};g.commands.push(c);pluginGroups.set(p.id,g)}
 const current=sub?pluginGroups.get(sub.id):undefined;
 const openSub=(id:string,el:HTMLButtonElement,focus=false)=>{const r=el.getBoundingClientRect(),w=Math.min(280,innerWidth-16),right=r.right+3,x=right+w<=innerWidth-8?right:Math.max(8,r.left-w-3);subTrigger.current=el;setSub({id,x,y:Math.max(8,Math.min(r.top,innerHeight-280))});if(focus)requestAnimationFrame(()=>subMenu.current?.querySelector<HTMLButtonElement>("button:not(:disabled)")?.focus())};
 const renderCommand=(c:CoreCommand,nested=false)=><button key={c.alias} type="button" onPointerEnter={()=>{if(!nested)setSub(null)}} role={nested?(c.pluginMenu?.checked!==undefined?"menuitemcheckbox":"menuitem"):undefined} aria-checked={nested?c.pluginMenu?.checked:undefined} disabled={c.disabled} title={c.disabled?commandDisabledReason(c):c.hint??c.label} onClick={()=>{close();window.dispatchEvent(new CustomEvent("ovg:menu-command",{detail:c.alias}))}}><span className="core-menu-command">{nested&&<span className="ovg-menu-check" aria-hidden="true">{c.pluginMenu?.checked?"✓":""}</span>}<b>{nested?c.pluginMenu?.label??c.label:c.label}</b>{(c.hint||group==="Tìm lệnh")&&<small>{c.hint??c.group}</small>}</span>{!nested&&!c.pluginMenu&&<code title={c.alias}>{c.alias}</code>}</button>;
 return <div className="core-ribbon" ref={root}><div className="core-ribbon-group core-ribbon-search-trigger"><button type="button" aria-expanded={group==="Tìm lệnh"} onClick={e=>open(e,"Tìm lệnh")}><span aria-hidden="true">⌕ </span>Tìm công cụ</button></div>{groups.map(g=><div key={g} className="core-ribbon-group"><button type="button" className={group===g?"active":""} aria-expanded={group===g} onClick={e=>open(e,g)}>{g}</button></div>)}{enabled&&group&&createPortal(<div ref={menu} tabIndex={-1} onKeyDown={menuKeys} className={"core-ribbon-menu core-ribbon-menu-fixed"+(group==="Tìm lệnh"?"":" is-dropdown")} style={{left:pos.x,top:pos.y,maxHeight:Math.max(160,innerHeight-pos.y-12)}} role="region" aria-label={"Menu "+group}><header><div><strong>{group}</strong><small>{menuTopics[group]??"Tìm nhanh mọi công cụ GIS Core"}</small></div><button aria-label="Đóng menu" onClick={()=>close(true)}>×</button></header><div className="core-menu-search"><input ref={search} aria-label="Tìm công cụ" placeholder="Tìm chức năng hoặc mã lệnh…" value={query} onChange={e=>setQuery(e.target.value)} /><small>{items.length} lệnh · {items.filter(c=>!c.disabled).length} khả dụng</small></div><div className="core-menu-items">{items.filter(c=>group!=="Plugin"||!c.pluginMenu).map(c=>renderCommand(c))}{pluginGroups.size>0&&<div className="ovg-menu-separator"/>}{[...pluginGroups].map(([id,p])=><button key={id} className="ovg-plugin-sub-trigger" type="button" aria-haspopup="menu" aria-expanded={sub?.id===id} onPointerEnter={e=>{if(sub?.id!==id)openSub(id,e.currentTarget)}} onClick={e=>openSub(id,e.currentTarget,true)} onKeyDown={e=>{if(e.key==="ArrowRight"){e.preventDefault();openSub(id,e.currentTarget,true)}}}><span className="core-menu-command"><b>{p.name}</b></span><span aria-hidden="true">▸</span></button>)}{!items.length&&<p>Không tìm thấy công cụ.</p>}</div>{current&&sub&&<div ref={subMenu} className="ovg-plugin-submenu" role="menu" aria-label={current.name} style={{left:sub.x,top:sub.y,maxHeight:Math.max(80,innerHeight-sub.y-8)}}>{current.commands.map((c,i)=><React.Fragment key={c.alias}>{i>0&&c.pluginMenu?.section!==current.commands[i-1].pluginMenu?.section&&<div className="ovg-menu-separator"/>}{renderCommand(c,true)}</React.Fragment>)}</div>}</div>,document.body)}</div>
}
