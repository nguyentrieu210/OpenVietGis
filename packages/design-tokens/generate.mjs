import {readFileSync,writeFileSync,mkdirSync} from 'node:fs';
import {dirname,join} from 'node:path';
import {fileURLToPath,pathToFileURL} from 'node:url';
function contrast(a,b){const l=h=>[1,3,5].map(i=>parseInt(h.slice(i,i+2),16)/255).map(c=>c<=.04045?c/12.92:((c+.055)/1.055)**2.4).reduce((s,c,i)=>s+c*[.2126,.7152,.0722][i],0);let x=l(a),y=l(b);return (Math.max(x,y)+.05)/(Math.min(x,y)+.05)}
function validate(t){
 const fail=m=>{throw new Error(m)};
 const expected={color:["surface","panel","ink","muted","line","primary","on-primary","focus","line-strong","danger","success","warning"],density:["gap","inset","section-gap","field-gap","control-height","toolbar-gap","touch-target"],radius:["ui","sm","md","lg"],typography:["font-family","mono-family","xs","sm","body","heading"],motion:["fast","normal","slow","reduced"]};
 const keys=(obj,required,label)=>{if(!obj||Object.keys(obj).sort().join(",")!==[...required].sort().join(","))fail("Invalid keys: "+label)};
 const leaf=(x,type,unit)=>{if(!x||x.$type!==type)fail("Invalid token type "+type);let v=x.$value;if(type==="color"){if(!v||v.colorSpace!=="srgb"||v.alpha!==1||!/^#[0-9a-f]{6}$/.test(v.hex)||!Array.isArray(v.components)||v.components.length!==3||v.components.some((c,i)=>!Number.isFinite(c)||c<0||c>1||Math.round(c*255)!==parseInt(v.hex.slice(1+i*2,3+i*2),16)))fail("Invalid sRGB color")}else if(type==="fontFamily"){if(!Array.isArray(v)||!v.length||v.some(x=>typeof x!=="string"||!/^[A-Za-z0-9 -]+$/.test(x)))fail("Invalid font family")}else if(!v||v.unit!==unit||!Number.isInteger(v.value)||v.value<0||v.value>1000)fail("Invalid "+type+" value")};
 keys(t,["$description","color","density","radius","typography","motion"],"root");
 keys(t.color,["light","dark"],"color modes");keys(t.density,["compact","comfortable","touch"],"density modes");
 for(const [mode,g] of Object.entries(t.color)){keys(g,expected.color,mode);for(const x of Object.values(g))leaf(x,"color")}
 for(const [mode,g] of Object.entries(t.density)){keys(g,expected.density,mode);for(const x of Object.values(g))leaf(x,"dimension","px");if(g["control-height"].$value.value<g["touch-target"].$value.value)fail("Control smaller than target")}
 if(t.density.touch["touch-target"].$value.value<48)fail("Touch target below 48");
 for(const k of ["radius","typography","motion"]){keys(t[k],expected[k],k);for(const [n,x] of Object.entries(t[k])){if(k==="typography"&&n.endsWith("family"))leaf(x,"fontFamily");else leaf(x,k==="motion"?"duration":"dimension",k==="motion"?"ms":"px")}}
 if(t.motion.reduced.$value.value!==0)fail("Reduced motion must be zero");
 for(const [mode,g] of Object.entries(t.color)){
  for(const bg of ["surface","panel"])for(const fg of ["ink","muted","primary","danger","success","warning"])if(contrast(g[fg].$value.hex,g[bg].$value.hex)<4.5)fail("Text contrast "+mode+" "+fg+"/"+bg);
  if(contrast(g["on-primary"].$value.hex,g.primary.$value.hex)<4.5)fail("Primary text contrast "+mode);
  for(const bg of ["surface","panel"])for(const fg of ["focus","line-strong"])if(contrast(g[fg].$value.hex,g[bg].$value.hex)<3)fail("Non-text contrast "+mode+" "+fg+"/"+bg);
 }
 return t;
}
function generate(t){
 validate(t);const head="/* Generated from tokens.json; do not edit. */\n";
 const cssVal=x=>x.$type==="color"?x.$value.hex:x.$type==="fontFamily"?x.$value.map(f=>f.includes(" ")?JSON.stringify(f):f).join(", "):x.$value.value+x.$value.unit;
 const block=(sel,entries)=>sel+" {\n"+entries.map(([k,v])=>"  --ovg-ui-"+k+": "+v+";").join("\n")+"\n}\n";
 const entries=(g,p="")=>Object.entries(g).sort(([a],[b])=>a<b?-1:a>b?1:0).map(([k,v])=>[p+k,cssVal(v)]);
 const light=entries(t.color.light),dark=entries(t.color.dark);
 const common=[...entries(t.radius,"radius-"),...entries(t.typography,"type-"),...entries(t.motion,"motion-"),["radius","var(--ovg-ui-radius-ui)"]];
 const root=[...light,...entries(t.density.compact),...common];
 const css=head+block(":root",root)+block(":root[data-ovg-theme=dark]",dark)+"@media (prefers-color-scheme: dark) {\n"+block(":root[data-ovg-theme=system]",dark).trimEnd().split("\n").map(x=>"  "+x).join("\n")+"\n}\n"+["comfortable","touch"].map(k=>block(":root[data-ovg-density="+k+"]",entries(t.density[k]))).join("")+"@media (prefers-reduced-motion: reduce) {\n"+block(":root",["fast","normal","slow"].map(k=>["motion-"+k,"var(--ovg-ui-motion-reduced)"])).trimEnd().split("\n").map(x=>"  "+x).join("\n")+"\n}\n";
 const tailwind=head+"/* Import after the prefixed Tailwind theme; no Preflight. */\n@theme inline {\n"+[...light.map(([k])=>["color-ui-"+k,"var(--ovg-ui-"+k+")"]),...Object.keys(t.radius).sort().map(k=>["radius-ui"+(k==="ui"?"":"-"+k),"var(--ovg-ui-radius-"+k+")"]),...["xs","sm","body","heading"].map(k=>["text-ui-"+k,"var(--ovg-ui-type-"+k+")"]),["font-ui","var(--ovg-ui-type-font-family)"],["font-ui-mono","var(--ovg-ui-type-mono-family)"]].map(([k,v])=>"  --"+k+": "+v+";").join("\n")+"\n}\n";
 const id=k=>k.replace(/-([a-z])/g,(_,c)=>c.toUpperCase());
 const num=x=>x.$value.value;
 const dart="// Generated from tokens.json; do not edit.\nimport 'package:flutter/material.dart';\n\nenum OvgDensity { compact, comfortable, touch }\n\nclass OvgTokens {\n  OvgTokens._();\n"+Object.entries(t.color).map(([mode,g])=>"  static const Map<String, Color> "+mode+" = {\n"+Object.entries(g).map(([k,x])=>"    '"+k+"': Color(0xff"+x.$value.hex.slice(1)+"),").join("\n")+"\n  };").join("\n")+"\n"+Object.entries(t.density).map(([mode,g])=>"  static const Map<String, double> "+mode+" = {\n"+Object.entries(g).map(([k,x])=>"    '"+k+"': "+num(x)+",").join("\n")+"\n  };").join("\n")+"\n"+Object.entries(t.radius).map(([k,x])=>"  static const double radius"+id(k[0].toUpperCase()+k.slice(1))+" = "+num(x)+";").join("\n")+"\n"+Object.entries(t.typography).filter(([k])=>!k.endsWith("family")).map(([k,x])=>"  static const double text"+id(k[0].toUpperCase()+k.slice(1))+" = "+num(x)+";").join("\n")+"\n"+Object.entries(t.motion).map(([k,x])=>"  static const Duration motion"+id(k[0].toUpperCase()+k.slice(1))+" = Duration(milliseconds: "+num(x)+");").join("\n")+"\n  static const List<String> fontFamilies = "+JSON.stringify(t.typography["font-family"].$value).replace(/"/g,"'")+";\n  static Map<String, double> spacing(OvgDensity density) => switch (density) {\n    OvgDensity.compact => compact,\n    OvgDensity.comfortable => comfortable,\n    OvgDensity.touch => touch,\n  };\n  static Duration motion(Duration value, {required bool reduced}) => reduced ? motionReduced : value;\n  static ThemeData theme(Brightness brightness, {OvgDensity density = OvgDensity.touch}) {\n    final c = brightness == Brightness.dark ? dark : light;\n    final d = spacing(density);\n    final scheme = ColorScheme.fromSeed(seedColor: c['primary']!, brightness: brightness).copyWith(\n      primary: c['primary'], onPrimary: c['on-primary'], surface: c['surface'], onSurface: c['ink'],\n      onSurfaceVariant: c['muted'], outline: c['line-strong'], outlineVariant: c['line'], error: c['danger'],\n    );\n    final minimum = Size(d['touch-target']!, d['control-height']!);\n    final shape = RoundedRectangleBorder(borderRadius: BorderRadius.circular(radiusUi));\n    final base = ThemeData(useMaterial3: true, colorScheme: scheme, scaffoldBackgroundColor: c['surface'],\n      materialTapTargetSize: MaterialTapTargetSize.padded,\n      filledButtonTheme: FilledButtonThemeData(style: FilledButton.styleFrom(minimumSize: minimum, shape: shape)),\n      outlinedButtonTheme: OutlinedButtonThemeData(style: OutlinedButton.styleFrom(minimumSize: minimum, shape: shape)),\n      textButtonTheme: TextButtonThemeData(style: TextButton.styleFrom(minimumSize: minimum, shape: shape)),\n      inputDecorationTheme: InputDecorationTheme(border: OutlineInputBorder(borderRadius: BorderRadius.circular(radiusUi)), contentPadding: EdgeInsets.all(d['gap']!)),\n    );\n    return base.copyWith(textTheme: base.textTheme.copyWith(bodyMedium: base.textTheme.bodyMedium?.copyWith(fontSize: textBody), titleMedium: base.textTheme.titleMedium?.copyWith(fontSize: textHeading)));\n  }\n}\n";
 return {"generated/tokens.css":css,"generated/tailwind.css":tailwind,"flutter/lib/openvietgis_design_tokens.dart":dart};
}
export {contrast,validate,generate};
const root=fileURLToPath(new URL('./',import.meta.url));
export function run(check=false){
 const data=JSON.parse(readFileSync(join(root,'tokens.json'),'utf8'));
 for(const [name,content] of Object.entries(generate(data))){const dest=join(root,name);if(check){if(readFileSync(dest,'utf8')!==content)throw Error('Generated artifact drift: '+name)}else{mkdirSync(dirname(dest),{recursive:true});writeFileSync(dest,content)}}
}
if(process.argv[1]&&import.meta.url===pathToFileURL(process.argv[1]).href){if(process.argv.slice(2).some(x=>x!=='--check'))throw Error('Usage: node generate.mjs [--check]');run(process.argv.includes('--check'));}
