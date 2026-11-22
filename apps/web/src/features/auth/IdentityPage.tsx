import {
  ApiError,
  type CurrentUser,
  listIdentityLinks,
  listIdentityProviders,
  loadCsrfToken,
  logout,
  unlinkIdentity
} from "@openvietgis/client-sdk";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { Link, useNavigate } from "react-router-dom";

import { apiClient } from "../../app/api";
import { currentProtectedData, identityLinksKey, isAuthorizationLossStatus, protectedQueryOptions } from "../../app/protected-query";
import { FreshAccount } from "./FreshAccount";

function IdentityDetails({ account }: { account: CurrentUser }) {
  const queryClient = useQueryClient();
  const navigate = useNavigate();
  const linksKey = identityLinksKey(account.id);
  const links = useQuery({
    queryKey: linksKey,
    queryFn: () => listIdentityLinks(apiClient),
    ...protectedQueryOptions
  });
  const providers = useQuery({
    queryKey: ["identity-providers"],
    queryFn: () => listIdentityProviders(apiClient),
    ...protectedQueryOptions
  });

  const currentLinks = currentProtectedData(links);
  const currentProviders = currentProtectedData(providers);

  const unlink = useMutation({
    mutationFn: async (identityLinkId: string) => {
      await loadCsrfToken(apiClient);
      await unlinkIdentity(apiClient, identityLinkId);
    },
    onSuccess: async () => {
      // Unlinking the active identity also invalidates its server session.
      await Promise.all([
        queryClient.invalidateQueries({ queryKey: ["me"] }),
        queryClient.invalidateQueries({ queryKey: linksKey })
      ]);
    },
    onError: async (error) => {
      await Promise.all([
        queryClient.invalidateQueries({ queryKey: linksKey }),
        ...(error instanceof ApiError && isAuthorizationLossStatus(error.status)
          ? [queryClient.invalidateQueries({ queryKey: ["me"] })] : [])
      ]);
    }
  });

  const signOut = useMutation({
    mutationFn: async () => {
      await loadCsrfToken(apiClient);
      await logout(apiClient);
    },
    onSuccess: async () => {
      queryClient.clear();
      await navigate("/");
    },
    onError: async () => {
      // Even an ambiguous network failure may follow a committed logout.
      await Promise.all([
        queryClient.invalidateQueries({ queryKey: ["me"] }),
        queryClient.invalidateQueries({ queryKey: linksKey })
      ]);
    }
  });

  return (
    <main style={{ maxWidth: 800, margin: "40px auto", fontFamily: "system-ui, sans-serif" }}>
      <p>
        <Link to="/">← Tổ chức</Link>
      </p>
      <h1>Danh tính & phiên đăng nhập</h1>
      <p>
        {account.display_name}
        {account.email_snapshot ? " — " + account.email_snapshot : ""}
      </p>

      <h2>Danh tính đã liên kết</h2>
      {links.fetchStatus !== "idle" || links.isPending ? <p>Đang kiểm tra danh tính đã liên kết…</p> : !currentLinks ? <p role="alert">Không thể tải danh tính đã liên kết.</p> : null}
      <ul>
        {currentLinks?.map((link) => (
          <li key={link.id}>
            {link.issuer} / {link.subject}
            {currentLinks.length > 1 ? (
              <button type="button" disabled={unlink.isPending || signOut.isPending} onClick={() => unlink.mutate(link.id)}>
                Hủy liên kết
              </button>
            ) : null}
          </li>
        ))}
      </ul>

      <h2>Liên kết thêm danh tính</h2>
      {providers.fetchStatus !== "idle" || providers.isPending ? <p>Đang kiểm tra nhà cung cấp danh tính…</p> : !currentProviders ? <p role="alert">Không thể tải nhà cung cấp danh tính.</p> : null}
      {currentLinks && !signOut.isPending && !unlink.isPending && currentProviders?.map((provider) => (
        <p key={provider.id}>
          <a
            href={
              "/api/v1/auth/login/" +
              encodeURIComponent(provider.id) +
              "?mode=link&return_to=/identity"
            }
          >
            Liên kết {provider.display_name}
          </a>
        </p>
      ))}

      <button type="button" onClick={() => signOut.mutate()} disabled={signOut.isPending || unlink.isPending}>
        Đăng xuất
      </button>
      {unlink.error ? <p role="alert">Không thể hủy liên kết danh tính.</p> : null}
      {signOut.error ? <p role="alert">Không thể đăng xuất.</p> : null}
    </main>
  );
}

export function IdentityPage() {
  return <FreshAccount>{(account) => <IdentityDetails key={account.id} account={account} />}</FreshAccount>;
}
