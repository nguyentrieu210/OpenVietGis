import React,{useEffect,useMemo,useRef,useState} from "react";
import {createPortal} from "react-dom";
import {isStudioMapCanvas} from "./map-surface";
export type CoreCommand={alias:string;label:string;group:string;hint?:string;disabled?:boolean;run:(args:string)=>void|Promise<void>};
const normalize=(s:string)=>s.normalize("NFD").replace(/[\u0300-\u036f]/g,"").replace(/đ/g,"d").toUpperCase();
const aliases:Record<string,string>={L:"LINE",PL:"POLYGON",PO:"POINT",M:"MOVE",CO:"COPY",CP:"COPY",O:"OFFSET",RO:"ROTATE",SC:"SCALE",S:"STRETCH",TR:"TRIM",EX:"EXTEND",F:"FILLET",E:"ERASE",DI:"DIST",AA:"AREA",P:"PAN",Z:"ZOOM",U:"UNDO",OS:"OSNAP",QSELECT:"SELRECT",RE:"RELOAD"};
type Host={commands:CoreCommand[];cancel:()=>string;activePrompt?:string;finishActive:()=>string|undefined;inputActive:(value:string)=>string|undefined;shortcut:(key:"F3"|"F8")=>string|undefined};
const editable=(t:HTMLElement|null)=>!!t&&(t.isContentEditable||["INPUT","TEXTAREA","SELECT"].includes(t.tagName)||!!t.closest('[contenteditable="true"]'));
export function CoreCommandBar(host:Host){
 const [value,setValue]=useState(""),[history,setHistory]=useState<string[]>([]),[log,setLog]=useState<string[]>([]),[status,setStatus]=useState("Sẵn sàng"),[focused,setFocused]=useState(false),[help,setHelp]=useState(false),[showHistory,setShowHistory]=useState(false),[cursor,setCursor]=useState(-1),[highlight,setHighlight]=useState(-1),[pending,setPending]=useState<"ZOOM"|"PAN"|null>(null);
 const ref=useRef<HTMLInputElement>(null),root=useRef<HTMLDivElement>(null),epoch=useRef(0),last=useRef(""),running=useRef(false);
 const prefix=normalize(value.trim().split(/\s/)[0]??"");
 const matches=useMemo(()=>{const candidates=host.commands.filter(c=>help||prefix&&(normalize(c.alias).startsWith(prefix)||normalize(c.label).includes(prefix)||Object.entries(aliases).some(([a,b])=>b===c.alias&&a.startsWith(prefix))));return candidates.sort((a,b)=>Number(!!a.disabled)-Number(!!b.disabled)||Number(normalize(b.alias)===prefix||aliases[prefix]===b.alias)-Number(normalize(a.alias)===prefix||aliases[prefix]===a.alias)).slice(0,help?host.commands.length:8)},[host.commands,prefix,help]);
 const report=(line:string)=>{setStatus(line);setLog(l=>[...l,line].slice(-80))};
 const focus=()=>{if(!document.querySelector('[aria-modal="true"]'))ref.current?.focus({preventScroll:true});else requestAnimationFrame(()=>{if(!document.querySelector('[aria-modal="true"]'))ref.current?.focus({preventScroll:true})})};
 const clear=()=>{setValue("");setHelp(false);setHighlight(-1);setCursor(-1);setPending(null)};
 function escape(){epoch.current++;clear();setShowHistory(false);const result=host.cancel();report(result);focus()}
 async function execute(raw:string,fromMenu=false){
  if(running.current)return;
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
  const command=host.commands.find(c=>normalize(c.alias)===alias)??(!rest.length&&matches.length===1?matches[0]:undefined);
  if(!command){report("Không có lệnh "+token.toUpperCase()+" · HELP");setPending(null);return}
  if(command.disabled){report(command.alias+" · chưa đủ điều kiện hoặc quyền");setPending(null);return}
  const full=[command.alias,...rest].join(" ");if(!host.activePrompt||!["ZE","ZI","ZO","ZL","ZS","ZOOM","PAN","ORTHO","OSNAP","SNAP"].includes(command.alias))last.current=full;setHistory(h=>[full,...h.filter(x=>x!==full)].slice(0,50));setLog(l=>[...l,"Command: "+full].slice(-80));
  const seq=++epoch.current;clear();setFocused(false);running.current=true;
  try{await command.run(rest.join(" "));if(seq===epoch.current)report(command.alias+" · "+command.label)}catch(e){if(seq===epoch.current)report(command.alias+" · "+(e instanceof Error?e.message:"Không thể thực hiện"))}finally{running.current=false}
 }
 const latest=useRef({execute,escape,focus,clear,shortcut:host.shortcut});latest.current={execute,escape,focus,clear,shortcut:host.shortcut};
 useEffect(()=>{const key=(e:KeyboardEvent)=>{
  if(e.defaultPrevented||e.isComposing||e.repeat&&["Escape","F2","F3","F8"].includes(e.key))return;
  const t=e.target as HTMLElement|null,own=t===ref.current,modal=!!document.querySelector('[aria-modal="true"]'),otherForm=editable(t)&&!own,button=!!t?.closest("button,a,[role=button]");
  if(e.key==="Escape"){if(document.querySelector(".core-ribbon-menu-fixed")||modal)return;e.preventDefault();latest.current.escape();return}
  if(modal||otherForm||e.altKey)return;
  if((e.ctrlKey||e.metaKey)&&(!own||!ref.current?.value)&&["z","y"].includes(e.key.toLowerCase())){e.preventDefault();e.stopPropagation();void latest.current.execute(e.key.toLowerCase()==="y"||e.shiftKey?"REDO":"UNDO");return}
  if(e.ctrlKey||e.metaKey)return;
  if(e.key==="F2"){e.preventDefault();e.stopPropagation();setShowHistory(v=>!v);setHelp(false);return}
  if(e.key==="F3"||e.key==="F8"){e.preventDefault();e.stopPropagation();const result=latest.current.shortcut(e.key);if(result)report(result);return}
  if(own)return;
  if(e.key==="/"&&!button){e.preventDefault();latest.current.focus();return}
  if((e.key==="Enter"||e.key===" ")&&!button){e.preventDefault();e.stopPropagation();void latest.current.execute("");return}
  if(/^[a-zA-Z0-9?@.,+-]$/.test(e.key)){e.preventDefault();e.stopPropagation();setValue(e.key.toUpperCase());setCursor(-1);setHighlight(-1);setHelp(false);latest.current.focus()}
 };window.addEventListener("keydown",key,true);return()=>window.removeEventListener("keydown",key,true)},[]);
 useEffect(()=>{const run=(e:Event)=>{const alias=(e as CustomEvent<string>).detail;void latest.current.execute(alias,true)};window.addEventListener("ovg:menu-command",run);return()=>window.removeEventListener("ovg:menu-command",run)},[]);
 useEffect(()=>{const menu=(e:MouseEvent)=>{if(!isStudioMapCanvas(e.target)||document.querySelector('[aria-modal="true"]'))return;e.preventDefault();void latest.current.execute("");};document.addEventListener("contextmenu",menu);return()=>document.removeEventListener("contextmenu",menu)},[]);
 useEffect(()=>{if(highlight>=0)root.current?.querySelector('[data-highlight="true"]')?.scrollIntoView({block:"nearest"})},[highlight]);
 useEffect(()=>{const out=(e:PointerEvent)=>{if(root.current&&!root.current.contains(e.target as Node)){setFocused(false);setHelp(false);setShowHistory(false)}};document.addEventListener("pointerdown",out);return()=>document.removeEventListener("pointerdown",out)},[]);
 const open=focused&&(help||!!prefix&&!value.includes(" ")&&!pending)&&matches.length>0;
 const placeholder=pending==="ZOOM"?"Zoom: E / I / O / L / S / mức zoom":pending==="PAN"?"Kinh độ,vĩ độ":host.activePrompt??(status==="Sẵn sàng"?"Gõ lệnh · L, M, Z, DI…":status);
 return <div className="core-command" ref={root}>
 <form onSubmit={e=>{e.preventDefault();void execute(value)}}><span className="core-command-prompt" aria-hidden="true">⌘</span>
 <input ref={ref} aria-label="Dòng lệnh GIS" autoComplete="off" spellCheck={false} placeholder={placeholder} value={value} onFocus={()=>setFocused(true)} onBlur={()=>setFocused(false)} onChange={e=>{setValue(e.target.value);setFocused(true);setCursor(-1);setHighlight(-1);setHelp(false)}} onKeyDown={e=>{
  if(e.key==="Tab"&&open){e.preventDefault();const c=matches[Math.max(0,highlight)];if(c){setValue(c.alias);setHelp(false);setHighlight(-1)}}
  else if(e.key==="ArrowUp"||e.key==="ArrowDown"){e.preventDefault();if(open&&value.trim()&&cursor<0){setHighlight(i=>e.key==="ArrowDown"?(i+1)%matches.length:(i<=0?matches.length-1:i-1))}else{const i=e.key==="ArrowUp"?Math.min(history.length-1,cursor+1):Math.max(-1,cursor-1);setCursor(i);setValue(i<0?"":history[i]);setHelp(false);setHighlight(-1)}}
  else if(e.key==="Enter"&&open&&highlight>=0){e.preventDefault();void execute(matches[highlight].alias)}
  else if(e.key===" "&&!e.nativeEvent.isComposing&&(value.trim()===""||!value.trim().includes(" ")&&(pending||host.activePrompt||host.commands.some(c=>c.alias===normalize(value)||aliases[normalize(value)]===c.alias)||["HELP","?","ESC"].includes(normalize(value))))){e.preventDefault();void execute(value)}
 }}/>
 <button type="submit" aria-label="Thực thi lệnh" title="Thực thi · Enter">↵</button><button type="button" aria-label="Lịch sử lệnh (F2)" title="Lịch sử lệnh · F2" aria-expanded={showHistory} onClick={()=>{setShowHistory(v=>!v);setHelp(false)}}>≡</button>
 <button type="button" aria-label="Danh sách lệnh" title="Danh sách lệnh · HELP" onClick={()=>{setValue("");setHelp(v=>!v);setFocused(true);setShowHistory(false);focus()}}>?</button>
 </form>
 <div className="core-command-status" style={{position:"absolute",width:1,height:1,padding:0,overflow:"hidden",clipPath:"inset(50%)",whiteSpace:"nowrap"}}><small role="status">{status}</small></div>
 {showHistory&&<div className="core-command-history" aria-label="Lịch sử lệnh"><header><b>Lịch sử lệnh</b><button onClick={()=>setShowHistory(false)} aria-label="Đóng lịch sử lệnh">×</button></header>{log.length?log.map((l,i)=><div key={i}>{l}</div>):<p>Chưa có lệnh.</p>}</div>}
 {open&&!showHistory&&<div className="core-command-list" role="listbox" aria-label="Lệnh khả dụng">{matches.map((c,i)=><button type="button" key={c.alias} data-highlight={i===highlight} disabled={c.disabled} onMouseDown={e=>e.preventDefault()} onClick={()=>void execute(c.alias)}><code>{c.alias}</code><span>{c.label}<small>{c.group}{c.hint?" · "+c.hint:""}</small></span></button>)}</div>}
 </div>
}
const menuTopics:Record<string,string>={"Bản đồ":"Điều hướng, góc nhìn, chọn vùng và cấu hình bản đồ","Layer":"Nguồn dữ liệu, thuộc tính, quản lý lớp và nhãn","Biên tập":"Vẽ, sửa hình học, thuộc tính và hoàn nguyên","Đo đạc":"Đo chiều dài, diện tích và hoàn tất thao tác","Phân tích":"Truy vấn, xử lý không gian và thống kê","CAD":"Công cụ hình học chính xác, snapping và DXF","Xuất bản":"Nhập, xuất, ký hiệu và bố cục in","Khác":"Nguồn ngoài, cộng tác và plugin"};
const needs:Record<string,string>={MOVE:"Chọn một điểm có quyền sửa hình học.",POINT:"Chọn layer điểm có quyền tạo đối tượng.",LINE:"Chọn layer đường có quyền tạo đối tượng.",POLYGON:"Chọn layer vùng có quyền tạo đối tượng.",ZL:"Chọn layer trong cây lớp.",ZS:"Chọn một đối tượng trên bản đồ.",LAYER:"Chọn layer trong cây lớp.",VERTEX:"Chọn đối tượng hình học có quyền sửa.",ERASE:"Chọn đối tượng có quyền xóa.",END:"Bắt đầu phép đo hoặc bản vẽ trước.",ESC:"Chỉ dùng khi có thao tác đang thực hiện.",LAYOUT:"Chờ bản đồ sẵn sàng.",DESELECT:"Chưa có đối tượng được chọn."};
export function CoreRibbon({commands}:{commands:CoreCommand[]}){
 const [group,setGroup]=useState(""),[query,setQuery]=useState(""),[pos,setPos]=useState({x:0,y:0});
 const groups=Object.keys(menuTopics),root=useRef<HTMLDivElement>(null),menu=useRef<HTMLDivElement>(null);
 const close=()=>{setGroup("");setQuery("")};
 useEffect(()=>{const outside=(e:PointerEvent)=>{if(!root.current?.contains(e.target as Node)&&!menu.current?.contains(e.target as Node))close()};const key=(e:KeyboardEvent)=>{if(e.key==="Escape"&&menu.current){e.stopImmediatePropagation();e.preventDefault();close()}};document.addEventListener("pointerdown",outside);window.addEventListener("keydown",key,true);window.addEventListener("resize",close);return()=>{document.removeEventListener("pointerdown",outside);window.removeEventListener("keydown",key,true);window.removeEventListener("resize",close)}},[]);
 const items=commands.filter(c=>(group==="Tìm lệnh"||c.group===group)&&(!query||normalize(c.alias+" "+c.label+" "+(c.hint??"")).includes(normalize(query))));
 const open=(e:React.MouseEvent<HTMLButtonElement>,g:string)=>{if(group===g){close();return}const r=e.currentTarget.getBoundingClientRect();setPos({x:Math.max(8,Math.min(r.left,innerWidth-Math.min(370,innerWidth-16)-8)),y:r.bottom+5});setQuery("");setGroup(g)};
 return <div className="core-ribbon" ref={root}><span className="core-ribbon-brand">GIS CORE</span>{groups.map(g=><div key={g} className="core-ribbon-group"><button type="button" className={group===g?"active":""} aria-expanded={group===g} onClick={e=>open(e,g)}>{g} <span>▾</span></button></div>)}<div className="core-ribbon-group"><button type="button" aria-expanded={group==="Tìm lệnh"} onClick={e=>open(e,"Tìm lệnh")}>⌕ Tìm lệnh</button></div>{group&&createPortal(<div ref={menu} className="core-ribbon-menu core-ribbon-menu-fixed" style={{left:pos.x,top:pos.y,maxHeight:Math.max(160,innerHeight-pos.y-12)}} aria-label={"Menu "+group}><header><div><strong>{group}</strong><small>{menuTopics[group]??"Tìm nhanh mọi công cụ GIS Core"}</small></div><button aria-label="Đóng menu" onClick={close}>×</button></header><div className="core-menu-search"><input aria-label="Tìm công cụ" placeholder="Tìm chức năng hoặc mã lệnh…" value={query} onChange={e=>setQuery(e.target.value)} /><small>{items.length} lệnh · {items.filter(c=>!c.disabled).length} khả dụng</small></div><div className="core-menu-items">{items.map(c=><button key={c.group+c.alias} type="button" disabled={c.disabled} title={c.disabled?needs[c.alias]??"Cần chọn layer/đối tượng, quyền phù hợp hoặc hoàn tất công cụ đang mở.":c.hint} onClick={()=>{close();window.dispatchEvent(new CustomEvent("ovg:menu-command",{detail:c.alias}))}}><span className="core-menu-command"><b>{c.label}</b><small>{c.disabled?needs[c.alias]??"Chưa đủ quyền hoặc điều kiện thao tác":c.hint??(group==="Tìm lệnh"?c.group:"Mở công cụ / thực thi")}</small></span><code>{c.alias}</code></button>)}{!items.length&&<p>Không tìm thấy công cụ.</p>}</div></div>,document.body)}</div>
}
