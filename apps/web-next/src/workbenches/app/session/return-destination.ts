export interface ReturnDestination {
  pathname: string;
  search: string;
  hash: string;
}

function browserBasePath(): string {
  const base = import.meta.env.BASE_URL || "/";
  return base === "/" ? "" : base.replace(/\/$/, "");
}

export function canonicalAppPath(): string {
  return browserBasePath() + "/app";
}

export function currentBrowserDestination(): string {
  if (typeof window === "undefined") return "/";
  return window.location.pathname + window.location.search + window.location.hash;
}

export function parseSafeReturnDestination(value: string | null): ReturnDestination | null {
  if (
    !value ||
    !value.startsWith("/") ||
    value.startsWith("//") ||
    value.includes("\\")
  ) {
    return null;
  }

  try {
    const base = "https://openvietgis.invalid";
    const url = new URL(value, base);
    if (url.origin !== base) return null;
    return {
      pathname: url.pathname,
      search: url.search,
      hash: url.hash
    };
  } catch {
    return null;
  }
}

export function safeReturnTo(value: string | null): string {
  const parsed = parseSafeReturnDestination(value);
  if (!parsed) return canonicalAppPath();
  return parsed.pathname + parsed.search + parsed.hash;
}
