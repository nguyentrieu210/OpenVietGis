import { describe, expect, it, vi } from "vitest";
import { ApiClient } from "../src/http";
import { listManagedSessions, revokeManagedSession, sendDesktopCommand } from "../src/managed-sync";

describe("managed connector API", () => {
  it("uses one API prefix and correct methods, identifiers, and CSRF", async () => {
    const fetchMock = vi.fn<typeof fetch>(async () => Response.json({}));
    const client = new ApiClient({baseUrl:"https://example.test/api/v1",fetchImpl:fetchMock});
    await listManagedSessions(client,"org","project");
    await revokeManagedSession(client,"org","project","session","csrf",true);
    await sendDesktopCommand(client,"org","project","session",{tool_name:"inspect"},"csrf");
    const root="https://example.test/api/v1/organizations/org/projects/project/integration-hub/managed";
    expect(fetchMock.mock.calls.map(call=>call[0])).toEqual([
      root+"/sessions", root+"/sessions/session?bump_epoch=true",
      root+"/desktop-sessions/session/commands"
    ]);
    expect(fetchMock.mock.calls.map(call=>call[1]?.method)).toEqual(["GET","DELETE","POST"]);
    expect(new Headers(fetchMock.mock.calls[1]?.[1]?.headers).get("x-openvietgis-csrf")).toBe("csrf");
    expect(new Headers(fetchMock.mock.calls[2]?.[1]?.headers).get("x-openvietgis-csrf")).toBe("csrf");
    expect(JSON.parse(String(fetchMock.mock.calls[2]?.[1]?.body))).toEqual({tool_name:"inspect"});
  });
});
