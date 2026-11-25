/**
 * Protected reads may retain an older successful payload during a failed or
 * in-flight refetch. Never treat that cached payload as current authorization.
 */
export const protectedQueryOptions = {
  staleTime: 0,
  refetchOnMount: "always",
  refetchOnWindowFocus: "always",
  refetchOnReconnect: "always"
} as const;

interface ProtectedQueryResult<T> {
  data: T | undefined;
  error: unknown;
  fetchStatus: "idle" | "fetching" | "paused";
  isSuccess: boolean;
}

export function currentProtectedData<T>(result: ProtectedQueryResult<T>): T | undefined {
  if (result.fetchStatus !== "idle" || !result.isSuccess || result.error !== null) {
    return undefined;
  }
  return result.data;
}

/** Identity links are private to the current authenticated actor. */
export function identityLinksKey(accountId: string) {
  return ["identity-links", accountId] as const;
}

/** A protected mutation denial can invalidate the current authorization context. */
export function isAuthorizationLossStatus(status: number): boolean {
  return status === 401 || status === 403 || status === 404;
}
