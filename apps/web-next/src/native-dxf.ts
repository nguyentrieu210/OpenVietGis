// Bounded, loss-conscious ASCII DXF subset. No DWG/binary or external engine.
export type Kind="LINE"|"CIRCLE"|"POINT";
export type Shape={id:number;kind:string;layer:string;points:[number,number][];radius?:number;fields:number[];editable:boolean;closed?:boolean};
export type Drawing={lines:string[];eol:string;entitiesEnd:number;shapes:Shape[];unsupported:number;canAppend:boolean};
export const blankDxf="0\nSECTION\n2\nHEADER\n9\n$ACADVER\n1\nAC1009\n0\nENDSEC\n0\nSECTION\n2\nENTITIES\n0\nENDSEC\n0\nEOF\n";
const num=(v:string)=>{if(!/^[+-]?(?:\d+(?:\.\d*)?|\.\d+)(?:e[+-]?\d+)?$/i.test(v.trim()))throw Error("Tọa độ DXF không hợp lệ.");const n=Number(v);if(!Number.isFinite(n)||Math.abs(n)>1e9)throw Error("Tọa độ vượt giới hạn.");return n};
export function parseDxf(text:string):Drawing{
 if(text.length>4*1024*1024||/[^\x09\x0a\x0d\x20-\x7e]/.test(text))throw Error("Chỉ hỗ trợ DXF văn bản ASCII tối đa 4 MiB.");
 const eol=text.includes("\r\n")?"\r\n":"\n";
 if(text.replaceAll(eol,"").includes("\n")||text.replaceAll(eol,"").includes("\r"))throw Error("DXF có kiểu xuống dòng không đồng nhất.");
 const lines=text.split(eol);let count=lines.length;while(count&&lines[count-1]==="")count--;
 if(count%2||count<4||count>200000)throw Error("Cấu trúc DXF không hợp lệ.");
 const code=(i:number)=>lines[i]?.trim(),val=(i:number)=>lines[i+1]?.trim();
 let start=-1,end=-1,sections=0,version="",eof=false;
 for(let i=0;i<count;i+=2){if(!/^\d{1,4}$/.test(code(i)??""))throw Error("Mã nhóm DXF không hợp lệ.");if(code(i)==="9"&&val(i)==="$ACADVER"&&code(i+2)==="1")version=val(i+2)??"";
 if(code(i)==="0"&&val(i)==="SECTION"&&code(i+2)==="2"&&val(i+2)==="ENTITIES"){start=i+4;sections++}
 if(start>=0&&end<0&&code(i)==="0"&&val(i)==="ENDSEC")end=i;
 if(code(i)==="0"&&val(i)==="EOF")eof=i===count-2;}
 if(sections!==1||start<0||end<start||!eof)throw Error("Thiếu vùng ENTITIES hoặc EOF của DXF.");
 const shapes:Shape[]=[];let unsupported=0;
 for(let i=start;i<end;){if(code(i)!=="0")throw Error("Đối tượng DXF không hợp lệ.");let j=i+2;while(j<end&&code(j)!=="0")j+=2;
 const kind=val(i)??"",groups=new Map<string,number[]>();for(let k=i+2;k<j;k+=2){const c=code(k)!;const arr=groups.get(c)??[];arr.push(k+1);groups.set(c,arr)}
 const idx=(c:string)=>groups.get(c)?.[0];const n=(c:string,fallback=0)=>{const k=idx(c);return k===undefined?fallback:num(lines[k]!)};
 const field=(c:string)=>{const k=idx(c);if(k===undefined||groups.get(c)?.length!==1)throw Error("Thiếu hoặc trùng tọa độ DXF.");return k};
 const planar=n("30")===0&&n("31")===0&&n("38")===0&&n("39")===0&&n("210")===0&&n("220")===0&&n("230",1)===1&&n("67")===0&&(!idx("410")||val(idx("410")!-1)?.toLowerCase()==="model");
 try{
 if(!planar)throw Error("Nonplanar");
 const layer=idx("8")===undefined?"0":lines[idx("8")!]!.trim();
 if(kind==="LINE"||kind==="CIRCLE"||kind==="POINT"){
 const fields=[field("10"),field("20"),...(kind==="LINE"?[field("11"),field("21")]:kind==="CIRCLE"?[field("40")]:[])],v=fields.map(k=>num(lines[k]!));
 if(kind==="CIRCLE"&&v[2]!<=0)throw Error("Radius");
 shapes.push({id:i,kind,layer,points:kind==="LINE"?[[v[0]!,v[1]!],[v[2]!,v[3]!]]:[[v[0]!,v[1]!]],radius:kind==="CIRCLE"?v[2]:undefined,fields,editable:true});
 }else if(kind==="LWPOLYLINE"&&!(groups.get("42")??[]).some(k=>num(lines[k]!)!==0)&&!(groups.get("40")??[]).some(k=>num(lines[k]!)!==0)&&!(groups.get("41")??[]).some(k=>num(lines[k]!)!==0)){
 const xs=groups.get("10")??[],ys=groups.get("20")??[];if(xs.length<2||xs.length!==ys.length||xs.length>20000)throw Error("Vertices");
 shapes.push({id:i,kind,layer,points:xs.map((k,k2)=>[num(lines[k]!),num(lines[ys[k2]!]!)]),fields:[],editable:false,closed:(n("70")&1)!==0});
 }else unsupported++;
 }catch{unsupported++}i=j;if(shapes.length>5000)throw Error("Quá 5.000 đối tượng hiển thị.");
 }
 // Only append to our simple R12 envelope: preserve arbitrary imported metadata.
 const prefix=lines.slice(0,start).join("\n"),suffix=lines.slice(end,count).join("\n");
 const template=blankDxf.trimEnd().split("\n");
 const canAppend=version==="AC1009"&&unsupported===0&&prefix===template.slice(0,14).join("\n")&&suffix==="0\nENDSEC\n0\nEOF"&&shapes.every(s=>s.editable&&s.layer==="0"&&(()=>{const allowed=new Set(["8","10","20","30",...(s.kind==="LINE"?["11","21","31"]:s.kind==="CIRCLE"?["40"]:[])]);let j=s.id+2;for(;j<end&&code(j)!=="0";j+=2){if(!allowed.has(code(j)!))return false}return true})());
 return {lines,eol,entitiesEnd:end,shapes,unsupported,canAppend};
}
export function editDxf(d:Drawing,shape:Shape,values:number[]):string{
 if(!d.canAppend||!shape.editable||d.unsupported||values.length!==shape.fields.length||!d.shapes.includes(shape))throw Error("Đối tượng không hỗ trợ chỉnh sửa an toàn.");
 const v=values.map(x=>num(String(x)));if(shape.kind==="CIRCLE"&&v[2]!<=0)throw Error("Bán kính phải lớn hơn 0.");
 if(shape.kind==="LINE"&&v[0]===v[2]&&v[1]===v[3])throw Error("Đường thẳng phải có chiều dài.");
 const lines=[...d.lines];shape.fields.forEach((k,i)=>lines[k]=String(v[i]));return lines.join(d.eol);
}
export function appendDxf(d:Drawing,kind:Kind,values:number[]):string{
 if(!d.canAppend)throw Error("Chỉ thêm hình vào bản vẽ DXF R12 đơn giản do công cụ tạo.");
 const v=values.map(x=>num(String(x)));if(v.length!==(kind==="LINE"?4:kind==="CIRCLE"?3:2))throw Error("Thiếu tọa độ.");
 if(kind==="CIRCLE"&&v[2]!<=0)throw Error("Bán kính phải lớn hơn 0.");
 if(kind==="LINE"&&v[0]===v[2]&&v[1]===v[3])throw Error("Đường thẳng phải có chiều dài.");
 const pairs=["0",kind,"8","0","10",String(v[0]),"20",String(v[1]),"30","0"];
 if(kind==="LINE")pairs.push("11",String(v[2]),"21",String(v[3]),"31","0");if(kind==="CIRCLE")pairs.push("40",String(v[2]));
 const lines=[...d.lines];lines.splice(d.entitiesEnd,0,...pairs);return lines.join(d.eol);
}

export type DxfView={x:number;y:number;w:number;h:number};
export function visibleDxfShapes(shapes:readonly Shape[],view:DxfView):Shape[]{
 const right=view.x+view.w,bottom=view.y+view.h;
 return shapes.filter(shape=>{
  let left=Infinity,top=Infinity,r=-Infinity,b=-Infinity;
  const radius=shape.radius??0;
  for(const [x,y0] of shape.points){const y=-y0;left=Math.min(left,x-radius);r=Math.max(r,x+radius);top=Math.min(top,y-radius);b=Math.max(b,y+radius)}
  return Number.isFinite(left)&&r>=view.x&&left<=right&&b>=view.y&&top<=bottom;
 });
}
