export type CommandContext={scope:string;locale:'vi'|'en';capabilities:ReadonlySet<string>};
export type Command={id:string;labels:Record<'vi'|'en',string>;group:string;shortcut?:string;capability?:string;enabled?:(c:CommandContext)=>boolean;run:(c:CommandContext)=>void|Promise<void>};
const idPattern=/^[a-z][a-z0-9_.-]{0,95}$/;
export function normalizeShortcut(s:string){const p=s.toLowerCase().split('+').map(x=>x.trim());const key=p.pop();if(!key||!/^([a-z0-9]|f[1-9]|f1[0-2]|enter|escape|space|delete|arrow(up|down|left|right))$/.test(key)||p.some(x=>!['ctrl','alt','shift','meta'].includes(x))||new Set(p).size!==p.length)throw Error('Invalid shortcut');return [...p.sort(),key].join('+')}
export class CommandRegistry{
 private commands=new Map<string,Command>();private shortcuts=new Map<string,string>();private active=new Set<string>();
 register(c:Command){if(!idPattern.test(c.id)||this.commands.has(c.id)||!c.labels.en.trim()||!c.labels.vi.trim())throw Error('Invalid command');const shortcut=c.shortcut?normalizeShortcut(c.shortcut):undefined;if(shortcut&&this.shortcuts.has(shortcut))throw Error('Shortcut conflict');this.commands.set(c.id,{...c,labels:{...c.labels},shortcut});if(shortcut)this.shortcuts.set(shortcut,c.id);return ()=>{if(this.active.has(c.id))throw Error('Command running');this.commands.delete(c.id);if(shortcut)this.shortcuts.delete(shortcut)}}
 reason(id:string,c:CommandContext){const cmd=this.commands.get(id);if(!cmd)return 'unknown';if(cmd.capability&&!c.capabilities.has(cmd.capability))return 'forbidden';if(this.active.has(id)||cmd.enabled?.(c)===false)return 'disabled';return null}
 list(c:CommandContext){return [...this.commands.values()].map(x=>({id:x.id,label:x.labels[c.locale],group:x.group,shortcut:x.shortcut,reason:this.reason(x.id,c)}))}
 async execute(id:string,context:()=>CommandContext){const c=context();if(this.reason(id,c))throw Error('Command denied');const cmd=this.commands.get(id)!;this.active.add(id);try{await cmd.run(c)}finally{this.active.delete(id)}}
 async keyboard(e:KeyboardEvent,context:()=>CommandContext){if(e.isComposing||e.repeat||e.defaultPrevented||(e.target instanceof Element&&e.target.closest('input,textarea,select,[contenteditable="true"],[role="textbox"],[aria-modal="true"]')))return false;let key:string;try{key=normalizeShortcut([e.ctrlKey?'ctrl':'',e.altKey?'alt':'',e.shiftKey?'shift':'',e.metaKey?'meta':'',e.key===' '?'space':e.key].filter(Boolean).join('+'))}catch{return false}const id=this.shortcuts.get(key);if(!id||this.reason(id,context()))return false;e.preventDefault();await this.execute(id,context);return true}
}
// Adapter for the existing CoreCommand registry; retain alias/argument behavior in the host.
export function coreCommandViews(registry:CommandRegistry,context:()=>CommandContext){
 return registry.list(context()).map(c=>({alias:c.id.toUpperCase(),label:c.label,group:c.group,disabled:!!c.reason,disabledReason:c.reason??undefined,run:async(args:string)=>{if(args.trim())throw Error('Unexpected arguments');await registry.execute(c.id,context)}}));
}
export {CommandPalette} from './palette';

export * from './discovery';