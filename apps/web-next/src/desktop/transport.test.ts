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
  it("decodes a bounded binary viewport, not a full-layer payload",async()=>{
    const data={generation:"1",data:{type:"FeatureCollection",features:[]},truncated:false};
    const invoke=vi.fn().mockResolvedValue(new TextEncoder().encode(JSON.stringify(data)).buffer);
    vi.stubGlobal("window",{__TAURI__:{core:{invoke}}});
    expect(await desktop.viewport("workspace",[0,0,1,1])).toEqual(data);
    expect(invoke).toHaveBeenCalledWith("desktop_viewport",{handle:"workspace",bbox:[0,0,1,1]});
    invoke.mockResolvedValue(new ArrayBuffer(2*1024*1024+1));
    await expect(desktop.viewport("workspace",[0,0,1,1])).rejects.toThrow("RESOURCE_LIMIT");
  });
  it("fails closed without the desktop host",()=>{
    vi.stubGlobal("window",{});
    expect(()=>desktop.capabilities()).toThrow("DESKTOP_HOST_REQUIRED");
  });
});
