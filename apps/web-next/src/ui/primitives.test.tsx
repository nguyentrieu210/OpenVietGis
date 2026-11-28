import React from "react";
import {renderToStaticMarkup} from "react-dom/server";
import {describe,expect,it} from "vitest";
import {Actions,Button,Input,Select,Textarea} from "./primitives";
describe("shared UI primitives",()=>{
 it("keeps native button semantics and opt-in variants",()=>{
  const html=renderToStaticMarkup(<Actions><Button className="legacy" disabled>Cancel</Button><Button type="submit" variant="primary">Save</Button></Actions>);
  expect(html).toContain('type="button"');
  expect(html).toContain('type="submit"');
  expect(html).toContain('disabled=""');
  expect(html).toContain("ovg-ui-actions");
  expect(html).toContain("primary");
  expect(html).toContain("legacy");
 });
 it("preserves accessible names, form values and native validation",()=>{
  const html=renderToStaticMarkup(<form><Input aria-label="Name" defaultValue="Parcel" required/><Select aria-label="CRS" defaultValue="4326"><option value="4326">WGS84</option></Select><Textarea aria-label="Notes" defaultValue="Survey"/></form>);
  expect(html).toContain('aria-label="Name"');
  expect(html).toContain('required=""');
  expect(html).toContain('value="Parcel"');
  expect(html).toContain('selected=""');
  expect(html).toContain('aria-label="Notes"');
  expect(html).toContain(">Survey</textarea>");
  expect(html.match(/ovg-ui-input/g)).toHaveLength(3);
 });
});
