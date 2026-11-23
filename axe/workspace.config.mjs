// The workspace page's browser checks (workspace.spec.mjs, ledger #3967, and
// rearrange.spec.mjs, ledger #3952) as a Playwright test
// run over the demo door: `DEMO_BASE=<url> npx playwright test -c axe/workspace.config.mjs`.
// scripts/screenshots.sh starts the door and runs it. One worker, Chromium only, no retries: a
// check that passes on a second try is not a pass.
import { defineConfig } from "@playwright/test";

export default defineConfig({
  testDir: ".",
  testMatch: /(workspace|rearrange)\.spec\.mjs$/,
  workers: 1,
  retries: 0,
  forbidOnly: true,
  reporter: [["list"]],
  updateSnapshots: "none",
  use: { browserName: "chromium" },
});
