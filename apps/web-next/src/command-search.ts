// Pure command discovery; discovery must not implicitly execute a fuzzy match.
export type SearchableCommand = {alias:string;label:string;group:string;hint?:string;disabled?:boolean};
export const normalizeCommand = (value:string) => value.normalize("NFD").replace(/[\u0300-\u036f]/g,"").replace(/[đĐ]/g,"D").toUpperCase().trim();
export function searchCommands<T extends SearchableCommand>(commands:readonly T[],query:string,aliases:Readonly<Record<string,string>>,limit=8):T[]{
 const q=normalizeCommand(query), words=q.split(/\s+/).filter(Boolean);
 return commands.filter(c=>{
  const names=Object.entries(aliases).filter(([,target])=>target===c.alias).map(([name])=>name);
  const haystack=normalizeCommand([c.alias,c.label,c.group,c.hint??"",...names].join(" "));
  return words.every(word=>haystack.includes(word));
 }).sort((a,b)=>Number(!!a.disabled)-Number(!!b.disabled)||Number(normalizeCommand(b.alias)===q||aliases[q]===b.alias)-Number(normalizeCommand(a.alias)===q||aliases[q]===a.alias)).slice(0,limit);
}
export function resolveCommand<T extends SearchableCommand>(commands:readonly T[],token:string,aliases:Readonly<Record<string,string>>):T|undefined{
 const key=normalizeCommand(token);
 const alias=Object.hasOwn(aliases,key)?aliases[key]:key;
 return commands.find(c=>normalizeCommand(c.alias)===alias);
}
