import type {
  ExternalConnection,
  ReleaseCapability,
  ReleaseCapabilityReport
} from "@openvietgis/client-sdk";

/** Never treat a dependency-health response or a legacy enabled flag as acceptance. */
function hasVerification(report: ReleaseCapabilityReport | undefined): boolean {
  return Boolean(
    report?.enabled_semantics === "release_certification"
    && report.verified_candidate
    && report.verification_record
  );
}

export function releaseProfileLabel(report: ReleaseCapabilityReport | undefined): string {
  return hasVerification(report) && report?.production_ready === true
    && report.profile_status === "accepted"
    ? "Đã nghiệm thu"
    : "Chưa nghiệm thu";
}

export function releaseStatusLabel(
  capability: ReleaseCapability,
  report: ReleaseCapabilityReport | undefined
): string {
  if (report?.enabled_semantics === "release_certification") {
    if (capability.status === "unsupported") return "Chưa hỗ trợ";
    if (capability.status === "disabled") return "Không nằm trong phạm vi phát hành";
    if (hasVerification(report) && capability.enabled && capability.status === "accepted") {
      return "Đã nghiệm thu";
    }
  }
  return "Chưa nghiệm thu";
}

export function externalAvailabilityLabel(
  connection: Pick<ExternalConnection, "lifecycle" | "availability">
): string {
  if (connection.lifecycle === "revoked" || connection.availability === "revoked") {
    return "Đã thu hồi";
  }
  return "Chưa xác minh · chỉ cấu hình";
}

export function entitlementTierLabel(tier: string | undefined): string {
  const labels: Record<string, string> = {
    self_host_community: "Cộng đồng tự triển khai",
    team_standard: "Nhóm tiêu chuẩn",
    enterprise: "Doanh nghiệp"
  };
  return tier ? labels[tier] ?? "Cấu hình khác" : "Chưa tải";
}
