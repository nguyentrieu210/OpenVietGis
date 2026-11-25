import type {
  ReleaseCapability,
  ReleaseCapabilityReport
} from "@openvietgis/client-sdk";
import { describe, expect, it } from "vitest";
import {
  entitlementTierLabel,
  externalAvailabilityLabel,
  releaseProfileLabel,
  releaseStatusLabel
} from "./platform-release-state";

const capability: ReleaseCapability = {
  id: "R010",
  name: "External sources",
  enabled: false,
  milestone: "M3",
  status: "unverified",
  reason: "exact_candidate_profile_acceptance_missing",
  required_for_team: false
};

const report: ReleaseCapabilityReport = {
  profile: "self_host_team",
  profile_status: "unverified",
  verified_candidate: null,
  verification_record: null,
  production_ready: false,
  enabled_semantics: "release_certification",
  total_releases: 15,
  accepted_release_count: 0,
  items: [capability]
};

describe("honest operations and provider status", () => {
  it("does not assert production acceptance for absent or unverified evidence", () => {
    expect(releaseProfileLabel(undefined)).toBe("Chưa nghiệm thu");
    expect(releaseProfileLabel(report)).toBe("Chưa nghiệm thu");
    expect(releaseStatusLabel(capability, report)).toBe("Chưa nghiệm thu");
  });

  it("rejects even optimistic enabled/status flags without a pinned verification", () => {
    const optimistic = { ...capability, enabled: true, status: "accepted" as const };
    expect(releaseStatusLabel(optimistic, undefined)).toBe("Chưa nghiệm thu");
    expect(releaseStatusLabel(optimistic, report)).toBe("Chưa nghiệm thu");
    expect(releaseProfileLabel({
      ...report, production_ready: true, profile_status: "accepted"
    })).toBe("Chưa nghiệm thu");
  });

  it("distinguishes unsupported, scope-disabled and unverified capabilities", () => {
    expect(releaseStatusLabel({ ...capability, status: "unsupported" }, report)).toBe("Chưa hỗ trợ");
    expect(releaseStatusLabel({ ...capability, status: "disabled" }, report))
      .toBe("Không nằm trong phạm vi phát hành");
  });

  it("does not advertise provider retrieval for configured or revoked connections", () => {
    expect(externalAvailabilityLabel({ lifecycle: "active", availability: "unverified" }))
      .toBe("Chưa xác minh · chỉ cấu hình");
    expect(externalAvailabilityLabel({ lifecycle: "active" }))
      .toBe("Chưa xác minh · chỉ cấu hình");
    expect(externalAvailabilityLabel({ lifecycle: "revoked" })).toBe("Đã thu hồi");
  });

  it("localizes configured entitlement tiers without inventing a default", () => {
    expect(entitlementTierLabel(undefined)).toBe("Chưa tải");
    expect(entitlementTierLabel("team_standard")).toBe("Nhóm tiêu chuẩn");
    expect(entitlementTierLabel("secret-new-tier")).toBe("Cấu hình khác");
  });
});
