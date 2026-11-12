// Spike (ledger #3545): one VoiceOver check, run as Guidepup's Playwright fixture expects:
// serially, headed, in WebKit. No retries, because the point is to measure how often it fails.
import { devices } from "@playwright/test";

export default {
  testDir: ".",
  testMatch: /.*\.spec\.mjs/,
  workers: 1,
  retries: 0,
  timeout: 5 * 60 * 1000,
  reporter: [["list"], ["json", { outputFile: "results/report.json" }]],
  use: { ...devices["Desktop Safari"], headless: false, video: "off" },
  projects: [{ name: "webkit", use: { ...devices["Desktop Safari"], headless: false } }],
};
