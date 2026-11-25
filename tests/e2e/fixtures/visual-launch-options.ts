import { fileURLToPath } from "node:url";
import type { LaunchOptions } from "@playwright/test";

// Use the baseline font set for screenshot tests without changing user fonts.
export const visualLaunchOptions: LaunchOptions = process.platform === "linux" ? {
  env: {
    ...process.env,
    FONTCONFIG_FILE: fileURLToPath(new URL("./visual-fonts.conf", import.meta.url))
  }
} : {};

// The reviewed Ubuntu/Debian font rasterization deltas were 25–109 pixels.
// Keep the allowance local to these visual suites and the 1440 px reference layouts.
export const visualScreenshotOptions = { animations: "disabled", maxDiffPixels: 128 } as const;
