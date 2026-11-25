import { ApiClient } from "@openvietgis/client-sdk";

export const apiClient = new ApiClient({
  baseUrl: import.meta.env.VITE_OPENVIETGIS_API_BASE_URL ?? "/api/v1"
});
