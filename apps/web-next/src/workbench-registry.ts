// Lazy registry for native workbench feature modules.
// All runtime source modules live in the canonical frontend.
import {lazy} from "react";
import "./workbenches/ui/tokens.css";
import "./workbenches/features/platform/platform.css";
export const Edit=lazy(()=>import("./workbenches/features/platform/EditingWorkbenchPage").then(m=>({default:m.EditingWorkbenchPage})));
export const Query=lazy(()=>import("./workbenches/features/platform/QueryWorkbenchPage").then(m=>({default:m.QueryWorkbenchPage})));
export const Process=lazy(()=>import("./workbenches/features/platform/ProcessingWorkbenchPage").then(m=>({default:m.ProcessingWorkbenchPage})));
export const Cartography=lazy(()=>import("./workbenches/features/platform/CartographyWorkbenchPage").then(m=>({default:m.CartographyWorkbenchPage})));
export const Hub=lazy(()=>import("./workbenches/features/platform/PlatformHubPage").then(m=>({default:m.PlatformHubPage})));
export const Field=lazy(()=>import("./workbenches/features/offline/OfflineFieldPage").then(m=>({default:m.OfflineFieldPage})));

export const CadImport=lazy(()=>import("./workbenches/features/platform/CadImportPanel").then(m=>({default:m.CadImportPanel})));
export const ExternalPreview=lazy(()=>import("./workbenches/features/platform/ExternalSourcePreview").then(m=>({default:m.ExternalSourcePreview})));
