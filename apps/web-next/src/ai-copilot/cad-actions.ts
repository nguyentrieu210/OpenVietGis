/** The CAD engine exposes a reviewed, scope-pinned host API, not arbitrary JS
 * or shell commands. Registration is removed when the drawing is closed. */
export type CadAiState={document_id:string;filename:string;version:number;dirty:boolean;saving:boolean;writable:boolean;selected_handle:string|null};
type Adapter={state:()=>CadAiState;select:(handle:string)=>boolean};
const adapters=new Map<string,Adapter>();
export function registerCadCopilot(path:string,adapter:Adapter){adapters.set(path,adapter);return()=>{if(adapters.get(path)===adapter)adapters.delete(path)}}
export function executeCadCopilot(org:string,pid:string,method:string,args:Record<string,unknown>){
 const path="/organizations/"+encodeURIComponent(org)+"/projects/"+encodeURIComponent(pid)+"/cad/documents";
 const adapter=adapters.get(path);
 if(!adapter)throw Error("Chưa có bản vẽ CAD sẵn sàng");
 const state=adapter.state();
 if(method==="cad_state")return state;
 if(method==="cad_select_handle"){
  if(!state.document_id||args.document_id!==state.document_id||args.version!==state.version)throw Error("Bản vẽ hoặc phiên bản CAD đã thay đổi");
  if(typeof args.handle!=="string"||! /^[0-9a-fA-F]{1,16}$/.test(args.handle)||!/[1-9a-f]/i.test(args.handle))throw Error("Handle không hợp lệ");
  if(state.saving||!adapter.select(args.handle))throw Error("Engine CAD chưa hỗ trợ chọn handle hoặc đang lưu");
  return {requested:true,document_id:state.document_id,version:state.version,handle:args.handle.toUpperCase()};
 }
 throw Error("Lệnh CAD chưa được hỗ trợ");
}
