import { ApiError, getCurrentUser, type CurrentUser } from "@openvietgis/client-sdk";
import { useQuery } from "@tanstack/react-query";
import type { ReactNode } from "react";

import { apiClient } from "../../app/api";
import { currentProtectedData } from "../../app/protected-query";
import { LoginPanel } from "./LoginPanel";
import "./auth.css";

export function FreshAccount({ children }: { children: (account: CurrentUser) => ReactNode }) {
  const user = useQuery({
    queryKey: ["me"],
    queryFn: () => getCurrentUser(apiClient),
    // The current account is session identity, not page data. Revalidating it
    // on every mount/focus caused the entire application shell to disappear
    // briefly because FreshAccount intentionally hides protected data while a
    // protected query is fetching. Authorization-sensitive project/layer
    // queries still use protectedQueryOptions independently.
    staleTime: 60_000,
    refetchOnMount: false,
    refetchOnWindowFocus: false,
    refetchOnReconnect: "always"
  });

  if (user.fetchStatus !== "idle" || user.isPending) {
    return <div className="ovg-auth-loading"><span className="ovg-login-logo">OVG</span><p>Đang kiểm tra phiên đăng nhập…</p></div>;
  }
  if (user.error instanceof ApiError && user.error.status === 401) return <LoginPanel />;

  const account = currentProtectedData(user);
  if (!account || account.status !== "active") {
    return <p role="alert">Không thể tải thông tin tài khoản.</p>;
  }
  return children(account);
}
