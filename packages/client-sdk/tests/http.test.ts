import { describe, expect, it } from "vitest";

import { ApiClient } from "../src/http";

describe("ApiClient security transport", () => {
  it("sends cookies and CSRF on unsafe requests", async () => {
    const calls: RequestInit[] = [];
    const client = new ApiClient({
      baseUrl: "http://example.test/api/v1",
      fetchImpl: (async (_input: RequestInfo | URL, init?: RequestInit) => {
        calls.push(init ?? {});
        return new Response(JSON.stringify({ ok: true }), {
          status: 200,
          headers: { "content-type": "application/json" }
        });
      }) as typeof fetch
    });
    client.setCsrfToken("csrf-token");

    await client.request("/organizations", {
      method: "POST",
      body: JSON.stringify({ name: "Org" })
    });

    expect(calls[0]?.credentials).toBe("include");
    const headers = new Headers(calls[0]?.headers);
    expect(headers.get("x-openvietgis-csrf")).toBe("csrf-token");
    expect(headers.get("content-type")).toBe("application/json");
  });

  it("does not add CSRF to safe requests", async () => {
    const calls: RequestInit[] = [];
    const client = new ApiClient({
      baseUrl: "http://example.test/api/v1",
      fetchImpl: (async (_input: RequestInfo | URL, init?: RequestInit) => {
        calls.push(init ?? {});
        return new Response(JSON.stringify({ ok: true }), { status: 200 });
      }) as typeof fetch
    });
    client.setCsrfToken("csrf-token");

    await client.request("/me");

    expect(new Headers(calls[0]?.headers).has("x-openvietgis-csrf")).toBe(false);
  });
});

describe("API path and error contract", () => {
  it("keeps a single configured base for REST and event URLs", () => {
    const client = new ApiClient({baseUrl:"/custom/api/v1/"});
    expect(client.url("/organizations/a/projects/b/events")).toBe("/custom/api/v1/organizations/a/projects/b/events");
    expect(client.url("/items?q=https%3A%2F%2Fexample.test")).toBe("/custom/api/v1/items?q=https%3A%2F%2Fexample.test");
  });

  it("rejects ambiguous or already-prefixed paths before sending", async () => {
    const calls: string[] = [];
    const client = new ApiClient({baseUrl:"/api/v1",fetchImpl:async input=>{
      calls.push(String(input));return Response.json({});
    }});
    for (const path of ["/api/v1/me","https://example.test/me","//example.test/me","/../me","/%2e%2e/me","/a#b","/a\\b"]) {
      await expect(client.request(path)).rejects.toThrow(TypeError);
    }
    expect(calls).toEqual([]);
  });

  it("normalizes error envelopes without consuming the original body", async () => {
    const {ApiError} = await import("../src/http");
    const response = Response.json({code:"INVALID",detail:[{msg:"Bad CRS",input:"hidden"},{bad:true}],message:"Invalid"}, {status:422});
    const error = new ApiError(422,response);
    const expected={code:"INVALID",message:"Invalid",validationMessages:["Bad CRS"]};
    expect(await error.readPayload()).toEqual(expected);
    expect(await error.readPayload()).toEqual(expected);
    expect(response.bodyUsed).toBe(false);
    expect((await response.json()).code).toBe("INVALID");
    expect(await new ApiError(502,new Response("bad gateway",{status:502})).readPayload()).toEqual({validationMessages:[]});
  });
});
