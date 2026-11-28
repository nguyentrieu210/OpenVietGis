import {afterEach,describe,expect,it,vi} from "vitest";
import {desktop} from "./transport";
describe("desktop transport",()=>{
  afterEach(()=>vi.unstubAllGlobals());
  it("uses the allowlisted command and typed argument names",async()=>{
    const invoke=vi.fn().mockResolvedValue("job-id");
    vi.stubGlobal("window",{__TAURI__:{core:{invoke}}});
    expect(await desktop.transform("workspace",4326,3857,[1,2])).toBe("job-id");
    expect(invoke).toHaveBeenCalledWith("desktop_start_transform",{handle:"workspace",source:4326,target:3857,xy:[1,2]});
    await desktop.cancel("workspace","job-id");
    expect(invoke).toHaveBeenCalledWith("desktop_cancel",{handle:"workspace",id:"job-id"});
  });
  it("rejects unbounded and invalid coordinate batches before IPC",()=>{
    const invoke=vi.fn();
    vi.stubGlobal("window",{__TAURI__:{core:{invoke}}});
    for(const xy of [[],[1],[NaN,0],Array(8194).fill(0)])expect(()=>desktop.transform("w",4326,3857,xy)).toThrow("INVALID_BATCH");
    expect(invoke).not.toHaveBeenCalled();
  });
  it("fails closed without the desktop host",()=>{
    vi.stubGlobal("window",{});
    expect(()=>desktop.capabilities()).toThrow("DESKTOP_HOST_REQUIRED");
  });
});
