import { fileURLToPath } from "node:url";
import type { LaunchOptions } from "@playwright/test";

// Use the baseline font set for screenshot tests without changing user fonts.
export const visualLaunchOptions: LaunchOptions = process.platform === "linux" ? {
  env: {
    ...process.env,
    FONTCONFIG_FILE: fileURLToPath(new URL("./visual-fonts.conf", import.meta.url))
  }
} : {};
