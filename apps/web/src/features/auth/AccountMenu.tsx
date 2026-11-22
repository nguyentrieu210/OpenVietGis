import {
  ApiError,
  getCurrentUser,
  loadCsrfToken,
  logout
} from "@openvietgis/client-sdk";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { Link, useNavigate } from "react-router-dom";

import { apiClient } from "../../app/api";
import { currentProtectedData } from "../../app/protected-query";
import "./account-menu.css";

export function AccountMenu() {
  const queryClient = useQueryClient();
  const navigate = useNavigate();
  const userQuery = useQuery({
    queryKey: ["me"],
    queryFn: () => getCurrentUser(apiClient),
    staleTime: 60_000,
    refetchOnMount: false,
    refetchOnWindowFocus: false,
    refetchOnReconnect: "always"
  });
  const account = currentProtectedData(userQuery);

  const signOut = useMutation({
    mutationFn: async () => {
      await loadCsrfToken(apiClient);
      await logout(apiClient);
    },
    onSuccess: async () => {
      queryClient.clear();
      await navigate("/");
    },
    onError: async (error) => {
      await queryClient.invalidateQueries({ queryKey: ["me"] });
      if (error instanceof ApiError && error.status === 401) {
        queryClient.clear();
        await navigate("/");
      }
    }
  });

  if (!account) return null;

  const initials = account.display_name
    .split(/\s+/)
    .filter(Boolean)
    .slice(-2)
    .map((part) => part[0]?.toUpperCase())
    .join("");

  return (
    <details className="ovg-account-menu">
      <summary aria-label="Mở menu tài khoản">
        <span className="ovg-account-avatar">{initials || "U"}</span>
        <span className="ovg-account-name">{account.display_name}</span>
        <span className="ovg-account-caret">⌄</span>
      </summary>
      <div className="ovg-account-dropdown">
        <div className="ovg-account-profile">
          <strong>{account.display_name}</strong>
          {account.email_snapshot ? <small>{account.email_snapshot}</small> : null}
        </div>
        <nav>
          <Link to="/">Trang làm việc</Link>
          <Link to="/identity">Tài khoản & phiên đăng nhập</Link>
          <Link to="/invitations/accept">Lời mời</Link>
        </nav>
        <button
          type="button"
          className="ovg-signout-button"
          disabled={signOut.isPending}
          onClick={() => signOut.mutate()}
        >
          {signOut.isPending ? "Đang đăng xuất…" : "Đăng xuất"}
        </button>
        {signOut.error ? <p role="alert">Không thể đăng xuất. Hãy thử lại.</p> : null}
      </div>
    </details>
  );
}
