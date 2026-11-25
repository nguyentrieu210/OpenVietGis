import {
  ApiError,
  getCurrentUser,
  type CurrentUser
} from "@openvietgis/client-sdk";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import {
  createContext,
  useContext,
  useEffect,
  useMemo,
  useRef,
  type ReactNode
} from "react";
import { Navigate, useLocation } from "react-router-dom";

import { apiClient } from "../api";
import { currentBrowserDestination } from "./return-destination";
import { observeOnlineActor } from "../../features/offline/offline-store";

interface SessionContextValue {
  account: CurrentUser;
  checking: boolean;
  networkWarning: boolean;
}

const SessionContext = createContext<SessionContextValue | null>(null);

export function useOptionalSession(): SessionContextValue | null {
  return useContext(SessionContext);
}

export function useSession(): SessionContextValue {
  const value = useOptionalSession();
  if (!value) throw new Error("useSession must be used inside SessionBoundary");
  return value;
}

export function SessionBoundary({ children }: { children: ReactNode }) {
  const queryClient = useQueryClient();
  const location = useLocation();
  const confirmedAccount = useRef<CurrentUser | null>(null);

  const user = useQuery({
    queryKey: ["me"],
    queryFn: () => getCurrentUser(apiClient),
    staleTime: 0,
    refetchOnMount: "always",
    refetchOnWindowFocus: "always",
    refetchOnReconnect: "always"
  });

  if (user.data?.status === "active") {
    confirmedAccount.current = user.data;
  }

  const unauthorized = user.error instanceof ApiError && user.error.status === 401;
  const account = unauthorized ? null : user.data?.status === "active"
    ? user.data
    : confirmedAccount.current;

  // Confirmed account changes or authorization loss lock the separate local field workspace.
  useEffect(() => {
    if (user.error instanceof ApiError && [401, 403].includes(user.error.status)) {
      observeOnlineActor(null);
    } else if (user.data && !user.error && user.fetchStatus === "idle") {
      observeOnlineActor(user.data.status === "active" ? user.data.id : null);
    }
  }, [user.data, user.error, user.fetchStatus]);

  useEffect(() => {
    if (!unauthorized) return;
    confirmedAccount.current = null;
    void queryClient.removeQueries({
      predicate: (query) => query.queryKey[0] !== "identity-providers"
    });
  }, [queryClient, unauthorized]);

  const value = useMemo<SessionContextValue | null>(() => {
    if (!account) return null;
    return {
      account,
      checking: user.fetchStatus === "fetching",
      networkWarning: Boolean(user.error && !unauthorized)
    };
  }, [account, unauthorized, user.error, user.fetchStatus]);

  if (unauthorized) {
    const returnTo = currentBrowserDestination();
    return (
      <Navigate
        replace
        to={"/login?return_to=" + encodeURIComponent(returnTo)}
        state={{ from: location.pathname }}
      />
    );
  }

  if (!value && (user.isPending || user.fetchStatus === "fetching")) {
    return (
      <main className="ovg-session-bootstrap" aria-busy="true">
        <span className="ovg-brand-mark">OVG</span>
        <p>Đang mở không gian làm việc…</p>
      </main>
    );
  }

  if (!value) {
    return (
      <main className="ovg-session-bootstrap" role="alert">
        <span className="ovg-brand-mark">OVG</span>
        <h1>Không thể xác nhận phiên đăng nhập</h1>
        <p>Kiểm tra kết nối rồi tải lại trang. Hệ thống chưa hiển thị dữ liệu được bảo vệ.</p>
      </main>
    );
  }

  return <SessionContext.Provider value={value}>{children}</SessionContext.Provider>;
}
