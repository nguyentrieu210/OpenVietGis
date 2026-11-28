export type ContextPin={mode:"gis"|"cad"|"3d";epoch:number;cad:string};
export type AiCommand={id:string;method:string;params:Record<string,unknown>;expires_in:number;scope:ContextPin;deadline:number};
export function modeAllows(mode:string,method:string):boolean;
export function validSession(s:unknown):boolean;
export function pinCommand(c:unknown,scope:ContextPin,now:number):AiCommand;
export function assertCommandCurrent(c:AiCommand,scope:ContextPin,now:number,minSeconds?:number):void;
export type HistoryEntry={id:string;method:string;mode:string;status:string;at:number};
export function recordHistory(history:HistoryEntry[],entry:HistoryEntry):HistoryEntry[];
