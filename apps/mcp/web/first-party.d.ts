export type MapBridge={view:()=>unknown;selection:()=>unknown;navigate:(target:{longitude:number;latitude:number;zoom:number})=>void;workspace?:(method:string,args:Record<string,unknown>,guard:()=>void)=>Promise<unknown>};
export function mountAiCopilot(host:HTMLElement, options:{organizationId:string;projectId:string;getBridge:()=>MapBridge|null}):()=>void;
