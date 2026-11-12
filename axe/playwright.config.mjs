// The accessibility-tree snapshots (aria.spec.mjs, ledger #3550) as a Playwright test run:
// `AXE_SITE=<site-dir> npx playwright test -c axe/playwright.config.mjs`, over the demo site
// `cargo run --example axe_site` writes. One worker, Chromium only, no retries: a snapshot that
// passes on a second try is not a pass.
import { defineConfig } from "@playwright/test";

export default defineConfig({
  testDir: ".",
  testMatch: "aria.spec.mjs",
  workers: 1,
  retries: 0,
  forbidOnly: true,
  reporter: [["list"]],
  // Any snapshot file next to the spec would be incidental text; every expectation is inline.
  updateSnapshots: "none",
  use: { browserName: "chromium" },
});
