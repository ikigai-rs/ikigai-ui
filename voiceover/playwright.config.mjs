// Real VoiceOver over the gallery (ledgers #4278 and #4279, from the #3545 spike), run as Guidepup's
// Playwright fixture expects: serially, headed, in WebKit. No retries: the job is informational
// (never a gate), and a retry would hide how often it fails.
import { devices } from "@playwright/test";

export default {
  testDir: ".",
  // The demo door, for workspace.spec.mjs (ledger #3953): see demo-door.mjs.
  globalSetup: "./demo-door.mjs",
  testMatch: /.*\.spec\.mjs/,
  workers: 1,
  retries: 0,
  timeout: 5 * 60 * 1000,
  reporter: [["list"], ["json", { outputFile: "results/report.json" }]],
  use: { ...devices["Desktop Safari"], headless: false, video: "off" },
  projects: [{ name: "webkit", use: { ...devices["Desktop Safari"], headless: false } }],
};
