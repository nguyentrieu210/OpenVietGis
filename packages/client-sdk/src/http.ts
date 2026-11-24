export interface ApiClientOptions {
  baseUrl: string;
  fetchImpl?: typeof fetch;
  credentials?: RequestCredentials;
}

const UNSAFE_METHODS = new Set(["POST", "PUT", "PATCH", "DELETE"]);

export interface ApiErrorPayload {
  code?: string;
  detail?: string;
  message?: string;
  validationMessages: string[];
}

export function projectApiPath(organizationId: string, projectId: string): string {
  return "/organizations/" + encodeURIComponent(organizationId) +
    "/projects/" + encodeURIComponent(projectId);
}

export class ApiError extends Error {
  private parsedPayload?: Promise<ApiErrorPayload>;

  constructor(
    public readonly status: number,
    public readonly response: Response
  ) {
    super(`OpenVietGIS API request failed: ${status}`);
    this.name = "ApiError";
  }

  /** Parse a clone once; retain the original response for compatibility. */
  readPayload(): Promise<ApiErrorPayload> {
    return this.parsedPayload ??= this.parsePayload();
  }

  private async parsePayload(): Promise<ApiErrorPayload> {
    const result: ApiErrorPayload = { validationMessages: [] };
    try {
      const data: unknown = await this.response.clone().json();
      if (!data || typeof data !== "object" || Array.isArray(data)) return result;
      const value = data as Record<string, unknown>;
      if (typeof value.code === "string") result.code = value.code;
      if (typeof value.detail === "string") result.detail = value.detail;
      if (typeof value.message === "string") result.message = value.message;
      if (Array.isArray(value.detail)) {
        result.validationMessages = value.detail.flatMap((entry: unknown) =>
          entry && typeof entry === "object" && "msg" in entry && typeof entry.msg === "string"
            ? [entry.msg] : []);
      }
    } catch {
      // Non-JSON responses and consumed bodies keep the generic status error.
    }
    return result;
  }
}

export class ApiClient {
  private readonly baseUrl: string;
  private readonly fetchImpl: typeof fetch;
  private readonly credentials: RequestCredentials;
  private csrfToken: string | undefined;

  constructor(options: ApiClientOptions) {
    this.baseUrl = options.baseUrl.replace(/\/$/, "");
    this.fetchImpl = options.fetchImpl ?? globalThis.fetch.bind(globalThis);
    this.credentials = options.credentials ?? "include";
  }

  setCsrfToken(token: string | undefined): void {
    this.csrfToken = token;
  }

  /** Paths are relative to the configured API base, including a leading slash. */
  url(path: string): string {
    const pathname = path.split("?")[0] ?? "";
    if (!pathname.startsWith("/") || pathname.startsWith("//") ||
        /[\\#\s]/.test(pathname) || path.includes("#") ||
        /^\/api\/v1(?:\/|$)/.test(pathname)) {
      throw new TypeError("Expected an API-relative path without /api/v1");
    }
    for (const segment of pathname.split("/")) {
      const decoded = decodeURIComponent(segment);
      if (decoded === "." || decoded === "..") {
        throw new TypeError("API path cannot contain traversal segments");
      }
    }
    return this.baseUrl + path;
  }

  private async send(path: string, init?: RequestInit): Promise<Response> {
    const method = (init?.method ?? "GET").toUpperCase();
    const headers = new Headers(init?.headers);
    if (init?.body !== undefined && !(init.body instanceof FormData) && !headers.has("content-type")) {
      headers.set("content-type", "application/json");
    }
    if (UNSAFE_METHODS.has(method) && this.csrfToken && !headers.has("x-openvietgis-csrf")) {
      headers.set("x-openvietgis-csrf", this.csrfToken);
    }

    const response = await this.fetchImpl(this.url(path), {
      ...init,
      method,
      credentials: init?.credentials ?? this.credentials,
      headers
    });

    if (!response.ok) {
      throw new ApiError(response.status, response);
    }

    return response;
  }

  async requestWithoutBody(path: string, init?: RequestInit): Promise<void> {
    await this.send(path, init);
  }

  async requestRaw(path: string, init?: RequestInit): Promise<Response> {
    return this.send(path, init);
  }

  async request<T>(path: string, init?: RequestInit): Promise<T> {
    const response = await this.send(path, init);
    if (response.status === 204) {
      return undefined as T;
    }
    return (await response.json()) as T;
  }
}