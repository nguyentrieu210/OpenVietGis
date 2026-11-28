// Spec 056: QGIS-like label expressions without code execution.
// "{field|fn:arg|fn}" placeholders; functions: upper, lower, round:n, number, default:x, prefix:x, suffix:x, if:x.
const MAX = 500;
type Attrs = Record<string, unknown>;

function apply(value: string, fn: string, arg: string | undefined): string {
  switch (fn) {
    case "upper": return value.toLocaleUpperCase("vi");
    case "lower": return value.toLocaleLowerCase("vi");
    case "round": { const n = Number(value.replace(",", ".")); const d = Math.max(0, Math.min(6, Number(arg ?? 0) || 0)); return value !== "" && Number.isFinite(n) ? n.toFixed(d) : value; }
    case "number": { const n = Number(value.replace(",", ".")); return value !== "" && Number.isFinite(n) ? n.toLocaleString("vi-VN", { maximumFractionDigits: 2 }) : value; }
    case "default": return value === "" ? arg ?? "" : value;
    case "prefix": return value === "" ? "" : (arg ?? "") + value;
    case "suffix": return value === "" ? "" : value + (arg ?? "");
    case "if": return value === "" ? "" : arg ?? value;
    default: return value; // unknown functions are ignored, never evaluated
  }
}

export function evalLabel(expr: string, attrs: Attrs): string {
  if(expr.startsWith("qgis:")){try{return evalQgisLabel(expr.slice(5),attrs).slice(0,4096)}catch{return ""}}
  return expr.slice(0, MAX).replace(/\{([^{}]+)\}/g, (_, body: string) => {
    const [field, ...pipes] = body.split("|");
    const key=(field??"").trim();
    let value = String(Object.hasOwn(attrs,key)?attrs[key]??"":"").trim();
    for (const p of pipes) {
      const k = p.indexOf(":");
      value = apply(value, (k < 0 ? p : p.slice(0, k)).trim(), k < 0 ? undefined : p.slice(k + 1));
    }
    return value;
  });
}

/** Validate the limited template language before saving; existing templates remain readable. */
export function validateLabel(expr:string,fields:readonly string[]):string|null{
 if(!expr.trim())return "Nhập biểu thức nhãn.";
 if(expr.length>MAX)return "Biểu thức tối đa 500 ký tự.";
 if(expr.startsWith("qgis:")){try{parseExpression(expr.slice(5),fields);return null}catch(e){return e instanceof Error?e.message:String(e)}}
 const known=new Set(fields);
 const pieces=expr.matchAll(/\{([^{}]*)\}/g);
 for(const match of pieces){
  const [raw,...pipes]=match[1]!.split("|"),field=raw!.trim();
  if(!known.has(field))return "Trường không tồn tại: "+field;
  for(const pipe of pipes){
   const i=pipe.indexOf(":"),fn=(i<0?pipe:pipe.slice(0,i)).trim(),arg=i<0?undefined:pipe.slice(i+1);
   if(!["upper","lower","round","number","default","prefix","suffix","if"].includes(fn))return "Hàm không hỗ trợ: "+fn;
   if(fn==="round"&&arg!==undefined&&!/^[0-6]$/.test(arg.trim()))return "round cần số nguyên 0–6.";
   if(["upper","lower","number"].includes(fn)&&arg!==undefined)return fn+" không nhận tham số.";
   if(["default","prefix","suffix","if"].includes(fn)&&arg===undefined)return fn+" cần tham số.";
  }
 }
 if(/[{}]/.test(expr.replace(/\{[^{}]*\}/g,"")))return "Ngoặc nhọn chưa cân bằng.";
 return null;
}

export type ExpressionValue=string|number|boolean|null;
type Ast={kind:"literal";value:ExpressionValue}|{kind:"field";name:string}|{kind:"unary";op:string;arg:Ast}|{kind:"binary";op:string;left:Ast;right:Ast}|{kind:"call";name:string;args:Ast[]};
type Token={kind:"field"|"string"|"number"|"word"|"symbol"|"eof";text:string};
const signatures:Record<string,[number,number]>={upper:[1,1],lower:[1,1],length:[1,1],trim:[1,1],to_string:[1,1],to_real:[1,1],round:[1,2],abs:[1,1],coalesce:[2,12],concat:[1,12],if:[3,3]};
const precedence:Record<string,number>={OR:1,AND:2,"=":3,"!=":3,"<>":3,"<":3,">":3,"<=":3,">=":3,IS:3,"||":4,"+":5,"-":5,"*":6,"/":6,"%":6};
function tokens(source:string):Token[]{
 if(source.length>MAX)throw Error("Biểu thức tối đa 500 ký tự.");
 const out:Token[]=[];let i=0;
 while(i<source.length){
  const rest=source.slice(i),ws=/^\s+/.exec(rest);if(ws){i+=ws[0].length;continue}
  const ch=rest[0]!;
  if(ch==="'"||ch==='"'){
   let s="",closed=false;i++;
   while(i<source.length){const c=source[i++]!;if(c===ch){if(source[i]===ch){s+=ch;i++;continue}closed=true;break}s+=c}
   if(!closed)throw Error("Chuỗi hoặc tên trường chưa đóng.");
   out.push({kind:ch==="'"?"string":"field",text:s});
  }else{
   const num=/^(?:\d+(?:\.\d*)?|\.\d+)(?:[eE][+-]?\d+)?/.exec(rest),word=/^[A-Za-z_][A-Za-z_0-9]*/.exec(rest),op=/^(?:\|\||<=|>=|<>|!=|[()+\-*/%,=<>])/.exec(rest);
   const m=num??word??op;if(!m)throw Error("Ký tự không hỗ trợ: "+ch);
   out.push({kind:num?"number":word?"word":"symbol",text:m[0]});i+=m[0].length;
  }
  if(out.length>160)throw Error("Biểu thức quá phức tạp.");
 }
 return [...out,{kind:"eof",text:""}];
}
export function parseExpression(source:string,fields?:readonly string[]):Ast{
 const ts=tokens(source),known=new Set(fields);let i=0,nodes=0;
 const peek=()=>ts[i]??ts.at(-1)!;
 const take=()=>{const t=peek();i++;return t};
 const word=(t:Token)=>t.kind==="word"?t.text.toUpperCase():t.kind==="symbol"?t.text:"";
 const consume=(s:string)=>word(peek())===s?(take(),true):false;
 const need=(s:string)=>{if(!consume(s))throw Error("Thiếu "+s)};
 function expression(min:number,depth:number):Ast{
  if(depth>24||++nodes>160)throw Error("Biểu thức quá sâu.");
  const t=take(),op=word(t);let left:Ast;
  if(op==="("){left=expression(1,depth+1);need(")")}
  else if(op==="+"||op==="-"||op==="NOT"){left={kind:"unary",op,arg:expression(op==="NOT"?3:7,depth+1)}}
  else if(t.kind==="number"){const value=Number(t.text);if(!Number.isFinite(value))throw Error("Số ngoài phạm vi.");left={kind:"literal",value}}
  else if(t.kind==="string")left={kind:"literal",value:t.text};
  else if(t.kind==="field"){if(fields&&!known.has(t.text))throw Error("Trường không tồn tại: "+t.text);left={kind:"field",name:t.text}}
  else if(["NULL","TRUE","FALSE"].includes(op))left={kind:"literal",value:op==="NULL"?null:op==="TRUE"};
  else if(t.kind==="word"){
   const name=t.text.toLowerCase(),sig=Object.hasOwn(signatures,name)?signatures[name]:undefined;if(!sig)throw Error("Hàm không hỗ trợ: "+name);
   need("(");const args:Ast[]=[];if(!consume(")")){do{if(args.length>=sig[1])throw Error("Quá nhiều tham số: "+name);args.push(expression(1,depth+1))}while(consume(","));need(")")}
   if(args.length<sig[0])throw Error("Thiếu tham số: "+name);left={kind:"call",name,args};
  }else throw Error("Thiếu toán hạng.");
  while(true){
   const p=word(peek()),level=Object.hasOwn(precedence,p)?precedence[p]:undefined;
   if(level===undefined||level<min)break;
   take();if(++nodes>160)throw Error("Biểu thức quá phức tạp.");
   if(p==="IS"){const not=consume("NOT");need("NULL");left={kind:"unary",op:not?"IS NOT NULL":"IS NULL",arg:left};continue}
   left={kind:"binary",op:p,left,right:expression(level+1,depth+1)};
  }
  return left;
 }
 const ast=expression(1,0);if(peek().kind!=="eof")throw Error("Ký hiệu thừa: "+peek().text);return ast;
}
const scalar=(x:unknown):ExpressionValue=>typeof x==="string"||typeof x==="boolean"||typeof x==="number"&&Number.isFinite(x)?x:null;
const numeric=(x:ExpressionValue):number=>{if(typeof x!=="number")throw Error("Cần toán hạng số; dùng to_real nếu trường là text.");return x};
const boolean=(x:ExpressionValue):boolean|null=>{if(x===null||typeof x==="boolean")return x;throw Error("Cần biểu thức boolean.")};
const checked=(n:number):number=>{if(!Number.isFinite(n))throw Error("Kết quả số không hữu hạn.");return n};
const bounded=(s:string):string=>{if(s.length>4096)throw Error("Kết quả text quá dài.");return s};
export function evaluateExpression(ast:Ast,attrs:Attrs):ExpressionValue{
 const run=(a:Ast):ExpressionValue=>{
  switch(a.kind){
   case "literal":return a.value;
   case "field":return Object.hasOwn(attrs,a.name)?scalar(attrs[a.name]):null;
   case "unary":{const x=run(a.arg);if(a.op==="IS NULL")return x===null;if(a.op==="IS NOT NULL")return x!==null;if(x===null)return null;if(a.op==="NOT")return !boolean(x);return checked(a.op==="-"?-numeric(x):numeric(x))}
   case "binary":{
    const x=run(a.left);
    if(a.op==="AND"){const b=boolean(x);if(b===false)return false;const y=boolean(run(a.right));return y===false?false:b===null||y===null?null:true}
    if(a.op==="OR"){const b=boolean(x);if(b===true)return true;const y=boolean(run(a.right));return y===true?true:b===null||y===null?null:false}
    const y=run(a.right);if(x===null||y===null)return null;
    if(a.op==="||")return bounded(String(x)+String(y));
    if(["=","!=","<>","<",">","<=",">="].includes(a.op)){
     if(typeof x!==typeof y)throw Error("Kiểu so sánh không khớp.");
     if(a.op==="=")return x===y;if(a.op==="!="||a.op==="<>")return x!==y;
     if(typeof x==="boolean"||typeof y==="boolean")throw Error("Boolean không có thứ tự.");
     const c=x<y?-1:x>y?1:0;return a.op==="<"?c<0:a.op===">"?c>0:a.op==="<="?c<=0:c>=0;
    }
    const l=numeric(x),r=numeric(y);if((a.op==="/"||a.op==="%")&&r===0)throw Error("Chia cho 0.");
    return checked(a.op==="+"?l+r:a.op==="-"?l-r:a.op==="*"?l*r:a.op==="/"?l/r:l%r);
   }
   case "call":{
    if(a.name==="coalesce"){for(const arg of a.args){const v=run(arg);if(v!==null)return v}return null}
    if(a.name==="if")return boolean(run(a.args[0]!))===true?run(a.args[1]!):run(a.args[2]!);
    const args=a.args.map(run),x=args[0]!;
    if(a.name==="concat")return bounded(args.map(v=>v===null?"":String(v)).join(""));
    if(x===null)return null;
    switch(a.name){
     case "upper":return bounded(String(x).toLocaleUpperCase("vi"));
     case "lower":return bounded(String(x).toLocaleLowerCase("vi"));
     case "trim":return bounded(String(x).trim());
     case "length":return [...String(x)].length;
     case "to_string":return bounded(String(x));
     case "to_real":{if(typeof x==="number")return x;if(typeof x!=="string"||!/^[-+]?(?:\d+(?:\.\d*)?|\.\d+)(?:e[-+]?\d+)?$/i.test(x.trim()))return null;const n=Number(x);return Number.isFinite(n)?n:null}
     case "abs":return Math.abs(numeric(x));
     case "round":{const digits=args.length===2?args[1]:0;if(digits===null)return null;if(typeof digits!=="number"||!Number.isInteger(digits)||digits<0||digits>6)throw Error("round cần số lẻ từ 0 đến 6.");return checked(Number(numeric(x).toFixed(digits)))}
     default:throw Error("Hàm không hỗ trợ.");
    }
   }
  }
 };
 return run(ast);
}
const expressionCache=new Map<string,Ast>();
export function evalQgisLabel(expr:string,attrs:Attrs):string{
 const key=expr;
 let ast=expressionCache.get(key);
 if(!ast){ast=parseExpression(expr);if(expressionCache.size>=128)expressionCache.delete(expressionCache.keys().next().value!);expressionCache.set(key,ast)}
 const result=evaluateExpression(ast,attrs);return result===null?"":String(result);
}
