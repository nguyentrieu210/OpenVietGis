// Sole transitional entrypoint to legacy frontend workbenches.
// Retire after parity and independent browser acceptance, not before.
import {lazy} from "react";
import "../../web/src/ui/tokens.css";
import "../../web/src/features/platform/platform.css";
export const Spatial=lazy(()=>import("../../web/src/features/spatial/SpatialCatalogPage").then(m=>({default:m.SpatialCatalogPage})));
export const Transfer=lazy(()=>import("../../web/src/features/transfers/ImportExportPage").then(m=>({default:m.ImportExportPage})));
export const Edit=lazy(()=>import("../../web/src/features/platform/EditingWorkbenchPage").then(m=>({default:m.EditingWorkbenchPage})));
export const Query=lazy(()=>import("../../web/src/features/platform/QueryWorkbenchPage").then(m=>({default:m.QueryWorkbenchPage})));
export const Process=lazy(()=>import("../../web/src/features/platform/ProcessingWorkbenchPage").then(m=>({default:m.ProcessingWorkbenchPage})));
export const Cartography=lazy(()=>import("../../web/src/features/platform/CartographyWorkbenchPage").then(m=>({default:m.CartographyWorkbenchPage})));
export const Hub=lazy(()=>import("../../web/src/features/platform/PlatformHubPage").then(m=>({default:m.PlatformHubPage})));
export const Field=lazy(()=>import("../../web/src/features/offline/OfflineFieldPage").then(m=>({default:m.OfflineFieldPage})));
