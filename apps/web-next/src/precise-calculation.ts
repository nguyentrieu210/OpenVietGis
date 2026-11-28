type Rational={n:bigint;d:bigint};
const abs=(x:bigint)=>x<0n?-x:x;
function rational(n:bigint,d=1n):Rational{
 if(d===0n)throw Error("Chia cho 0.");
 if(d<0n){n=-n;d=-d}
 let a=abs(n),b=d;while(b){const t=a%b;a=b;b=t}
 n/=a;d/=a;
 if(abs(n).toString().length>512||d.toString().length>512)throw Error("Công thức vượt giới hạn độ chính xác.");
 return {n,d};
}
function parse(raw:unknown):Rational{
 if(typeof raw!=="string"&&typeof raw!=="number")throw Error("Giá trị không phải số.");
 if(typeof raw==="number"&&!Number.isSafeInteger(raw))throw Error("Giá trị số không chính xác.");
 const s=String(raw);
 if(!/^[+-]?(?:\d+(?:\.\d*)?|\.\d+)$/.test(s)||s.length>80)throw Error("Giá trị số không hợp lệ.");
 const [i="",f=""]=s.replace(/^[+-]/,"").split(".");
 return rational(BigInt((i||"0")+f)*(s.startsWith("-")?-1n:1n),10n**BigInt(f.length));
}
const plus=(a:Rational,b:Rational,sign=1n)=>rational(a.n*b.d+sign*b.n*a.d,a.d*b.d);
const mul=(a:Rational,b:Rational)=>rational(a.n*b.n,a.d*b.d);
const div=(a:Rational,b:Rational)=>rational(a.n*b.d,a.d*b.n);
const compare=(a:Rational,b:Rational)=>a.n*b.d<b.n*a.d?-1:a.n*b.d>b.n*a.d?1:0;
function render(a:Rational,places:number,exactInteger=false){
 if(exactInteger&&a.n%a.d!==0n)throw Error("Kết quả không nguyên.");
 const scale=10n**BigInt(places),num=abs(a.n)*scale;
 let q=num/a.d;if(num%a.d*2n>=a.d)q++;
 const s=q.toString().padStart(places+1,"0"),whole=places?s.slice(0,-places):s,frac=places?s.slice(-places).replace(/0+$/,""):"";
 return (a.n<0n&&q!==0n?"-":"")+whole+(frac?"."+frac:"");
}
/** Arithmetic only; exact rational evaluation, explicit final half-away rounding. */
export function calculateExact(expr:string,attrs:Record<string,unknown>,numericFields:ReadonlySet<string>,kind:"integer"|"decimal"){
 if(expr.length>1024)throw Error("Công thức quá dài.");
 const tokens=expr.match(/\[[A-Za-z][A-Za-z0-9_]*\]|(?:\d+(?:\.\d*)?|\.\d+)|[+*/()\-]/g)??[];
 if(!tokens.length||tokens.length>120||tokens.join("")!==expr.replace(/\s+/g,""))throw Error("Công thức gồm số, [mã_trường], + - * / và ngoặc.");
 let i=0;
 const atom=():Rational=>{const t=tokens[i++];if(t==="-"||t==="+"){const a=atom();return t==="-"?rational(-a.n,a.d):a}if(t==="("){const v=sum();if(tokens[i++]!==")")throw Error("Thiếu dấu ngoặc.");return v}
  if(!t)throw Error("Thiếu toán hạng.");if(t.startsWith("[")){const id=t.slice(1,-1);if(!numericFields.has(id)||!Object.hasOwn(attrs,id))throw Error("Trường không phải số: "+id);return parse(attrs[id])}return parse(t)};
 const product=():Rational=>{let a=atom();while(tokens[i]==="*"||tokens[i]==="/"){const op=tokens[i++],b=atom();a=op==="*"?mul(a,b):div(a,b)}return a};
 const sum=():Rational=>{let a=product();while(tokens[i]==="+"||tokens[i]==="-"){const op=tokens[i++];a=plus(a,product(),op==="+"?1n:-1n)}return a};
 const value=sum();if(i!==tokens.length)throw Error("Công thức không hợp lệ.");
 return render(value,kind==="integer"?0:18,kind==="integer");
}
export type AttributeRecord={feature_id:string;attributes:Record<string,unknown>};
export function summarizeAttributes(rows:readonly AttributeRecord[],field:string,groupField:string,numeric:boolean){
 const groups=new Map<string,{label:string;count:number;valid:number;missing:number;sum:Rational;min:Rational|null;max:Rational|null}>();
 for(const row of rows){
  const a=row.attributes,v=groupField?a[groupField]:undefined,key=groupField?JSON.stringify([typeof v,v??null]):"all";
  const label=groupField?v===null?"(NULL)":v===undefined?"(Thiếu)":v===""?"(Chuỗi rỗng)":String(v):"Tất cả";
  const g=groups.get(key)??{label,count:0,valid:0,missing:0,sum:rational(0n),min:null,max:null};g.count++;
  if(a[field]===null||a[field]===undefined){g.missing++}else if(numeric){const x=parse(a[field]);g.valid++;g.sum=plus(g.sum,x);if(!g.min||compare(x,g.min)<0)g.min=x;if(!g.max||compare(x,g.max)>0)g.max=x}else g.valid++;
  groups.set(key,g);
 }
 return [...groups].sort((a,b)=>b[1].count-a[1].count||a[0].localeCompare(b[0])).map(([key,g])=>({key,label:g.label,count:g.count,valid:g.valid,missing:g.missing,sum:numeric&&g.valid?render(g.sum,18):null,mean:numeric&&g.valid?render(div(g.sum,rational(BigInt(g.valid))),18):null,min:g.min?render(g.min,18):null,max:g.max?render(g.max,18):null}));
}
