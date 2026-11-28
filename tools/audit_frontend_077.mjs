import {execFileSync} from 'node:child_process';
import {readFileSync} from 'node:fs';
import {fileURLToPath,pathToFileURL} from 'node:url';
const root=fileURLToPath(new URL('../',import.meta.url));
const baselineFile=new URL('../specs/077-universal-frontend-platform/baseline.json',import.meta.url);
const git=(args)=>execFileSync('git',args,{cwd:root,encoding:'utf8',maxBuffer:16*1024*1024});
export function summarize(entries){
 const blobs=entries.filter(e=>e.type==='blob').map(e=>e.path).sort();
 return {blobs:blobs.length,tsx:blobs.filter(p=>p.endsWith('.tsx')).length,css:blobs.filter(p=>p.endsWith('.css')).length,cssPaths:blobs.filter(p=>p.endsWith('.css'))};
}
export function parseTree(raw){
 return raw.split('\0').filter(Boolean).map(line=>{
  const i=line.indexOf('\t');if(i<0)throw Error('Malformed git tree entry');
  const [mode,type,sha]=line.slice(0,i).split(' ');if(!mode||!type||!sha)throw Error('Malformed git tree metadata');
  return {type,path:line.slice(i+1)};
 });
}
export function differences(scope,observed){
 const errors=[];
 for(const key of ['blobs','tsx','css'])if(scope[key]!==observed[key])errors.push(key+': expected '+scope[key]+', got '+observed[key]);
 if(scope.cssGroups){
  const paths=Object.values(scope.cssGroups).flat();
  if(new Set(paths).size!==paths.length)errors.push('Duplicate CSS ownership');
  const expected=new Set(paths),actual=new Set(observed.cssPaths);
  for(const p of expected)if(!actual.has(p))errors.push('Missing CSS: '+p);
  for(const p of actual)if(!expected.has(p))errors.push('Unclassified CSS: '+p);
 }
 return errors;
}
export function main(args){
 if(args.some(a=>!['--verify','--all'].includes(a)))throw Error('Usage: node tools/audit_frontend_077.mjs [--verify] [--all]');
 const verify=args.includes('--verify'),all=args.includes('--all');
 if(all&&!verify)throw Error('--all requires --verify for pinned branch snapshots');
 const data=JSON.parse(readFileSync(baselineFile,'utf8'));
 const scopes=all?data.scopes:data.scopes.slice(0,1);
 let failed=false;
 for(const scope of scopes){
  try{
   const ref=verify?scope.commit:'HEAD';
   const tree=git(['rev-parse',ref+':'+scope.path]).trim();
   const observed=summarize(parseTree(git(['ls-tree','-r','-z',tree])));
   const errors=differences(scope,observed);
   if(verify&&tree!==scope.tree)errors.push('Tree SHA mismatch');
   console.log(JSON.stringify({scope:scope.name,ref,tree,observed,errors}));
   if(verify&&errors.length)failed=true;
  }catch(error){console.error(scope.name+': '+error.message);failed=true;}
 }
 return failed?1:0;
}
if(process.argv[1]&&import.meta.url===pathToFileURL(process.argv[1]).href)process.exitCode=main(process.argv.slice(2));
