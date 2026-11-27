import { describe, expect, it } from "vitest";

import { parseSafeReturnDestination } from "./return-destination";

describe("return destination", () => {
  it("accepts an application-relative destination", () => {
    const result = parseSafeReturnDestination("/app/projects?q=map#layers");
    expect(result).toEqual({
      pathname: "/app/projects",
      search: "?q=map",
      hash: "#layers"
    });
  });

  it("rejects protocol-relative external destinations", () => {
    expect(parseSafeReturnDestination("//evil.example/path")).toBeNull();
  });

  it("rejects absolute external destinations", () => {
    expect(parseSafeReturnDestination("https://evil.example/path")).toBeNull();
  });
});
