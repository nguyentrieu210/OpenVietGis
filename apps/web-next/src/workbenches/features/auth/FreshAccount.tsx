import type { CurrentUser } from "@openvietgis/client-sdk";
import type { ReactNode } from "react";

import {
  SessionBoundary,
  useOptionalSession
} from "../../app/session/SessionBoundary";

function SessionAccount({
  children
}: {
  children: (account: CurrentUser) => ReactNode;
}) {
  const session = useOptionalSession();
  if (!session) return null;
  return children(session.account);
}

export function FreshAccount({
  children
}: {
  children: (account: CurrentUser) => ReactNode;
}) {
  const session = useOptionalSession();
  if (session) return children(session.account);

  return (
    <SessionBoundary>
      <SessionAccount>{children}</SessionAccount>
    </SessionBoundary>
  );
}
