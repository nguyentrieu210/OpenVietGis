import { createBrowserRouter } from "react-router-dom";

import { IdentityPage } from "../features/auth/IdentityPage";
import { OrganizationHome } from "../features/organizations/OrganizationHome";
import { ProjectPage } from "../features/projects/ProjectPage";
import { MembershipPage } from "../features/membership/MembershipPage";
import { InvitationAcceptPage } from "../features/invitations/InvitationAcceptPage";
import { WorkspacePage } from "../features/workspaces/WorkspacePage";

import { SpatialCatalogPage } from "../features/spatial/SpatialCatalogPage";
import { MapWorkspace } from "../features/map/MapWorkspace";
import { ImportExportPage } from "../features/transfers/ImportExportPage";
import { EditingWorkbenchPage } from "../features/platform/EditingWorkbenchPage";
import { QueryWorkbenchPage } from "../features/platform/QueryWorkbenchPage";
import { ProcessingWorkbenchPage } from "../features/platform/ProcessingWorkbenchPage";
import { CartographyWorkbenchPage } from "../features/platform/CartographyWorkbenchPage";
import { PlatformHubPage } from "../features/platform/PlatformHubPage";

const routerBasename = window.location.pathname === "/dev" || window.location.pathname.startsWith("/dev/")
  ? "/dev"
  : undefined;

export const router = createBrowserRouter([
  { path: "/", element: <OrganizationHome /> },
  { path: "/organizations/:organizationId", element: <OrganizationHome /> },
  { path: "/organizations/:organizationId/workspaces", element: <WorkspacePage /> },
  { path: "/organizations/:organizationId/projects", element: <ProjectPage /> },
  { path: "/organizations/:organizationId/projects/:projectId/spatial", element: <SpatialCatalogPage /> },
  { path: "/organizations/:organizationId/projects/:projectId/map", element: <MapWorkspace /> },
  { path: "/organizations/:organizationId/projects/:projectId/transfers", element: <ImportExportPage /> },
  { path: "/organizations/:organizationId/projects/:projectId/editing", element: <EditingWorkbenchPage /> },
  { path: "/organizations/:organizationId/projects/:projectId/queries", element: <QueryWorkbenchPage /> },
  { path: "/organizations/:organizationId/projects/:projectId/processing", element: <ProcessingWorkbenchPage /> },
  { path: "/organizations/:organizationId/projects/:projectId/cartography", element: <CartographyWorkbenchPage /> },
  { path: "/organizations/:organizationId/projects/:projectId/platform-hub", element: <PlatformHubPage /> },
  { path: "/organizations/:organizationId/membership", element: <MembershipPage /> },
  { path: "/invitations/accept", element: <InvitationAcceptPage /> },
  { path: "/identity", element: <IdentityPage /> }
], routerBasename ? { basename: routerBasename } : {});