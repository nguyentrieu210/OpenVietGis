import {describe,it,expect} from "vitest";
import {ApiError} from "@openvietgis/client-sdk";
import {isPermissionError} from "./SpatialIncremental";

describe("spatial authorization loss",()=>{
 it("fails closed on anti-enumeration not-found as well as auth denials",()=>{
  for(const status of [401,403,404])expect(isPermissionError(new ApiError(status,new Response("denied",{status})))).toBe(true);
  expect(isPermissionError(new ApiError(409,new Response("stale",{status:409})))).toBe(false);
  expect(isPermissionError(new Error("network"))).toBe(false);
 });
});
