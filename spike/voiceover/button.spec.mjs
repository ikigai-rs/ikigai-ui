// Spike (ledger #3545): VoiceOver reaches the gallery's "Sign up" button and says its name and
// role. The site is the one the axe job checks (`cargo run --example axe_site`), served by
// axe/serve.mjs with the door's CSP. The assertion is loose on purpose: name and role by
// pattern, never VoiceOver's exact wording, which changes with macOS and browser versions.
import { voiceOverTest as test } from "@guidepup/playwright";
import { expect } from "@playwright/test";
import { mkdir, writeFile } from "node:fs/promises";
import { serve } from "../../axe/serve.mjs";

const SITE = process.env.SITE ?? "../../target/axe-site";
const STEPS = 80;

test("VoiceOver announces the Sign up button's name and role", async ({ page, voiceOver }) => {
  const site = await serve(SITE);
  try {
    await page.goto(`${site.base}/gallery.html`, { waitUntil: "load" });
    await page.locator("main").waitFor();
    await voiceOver.navigateToWebContent();

    let found = false;
    for (let i = 0; i < STEPS && !found; i++) {
      await voiceOver.next();
      const item = (await voiceOver.itemText()) ?? "";
      found = /sign up/i.test(item) && /button/i.test(item);
    }
    const log = await voiceOver.spokenPhraseLog();
    await mkdir("results", { recursive: true });
    await writeFile("results/spoken.json", JSON.stringify(log, null, 2));

    expect(found, `no item matched "Sign up" + "button" in ${STEPS} steps`).toBe(true);
    expect(await voiceOver.lastSpokenPhrase()).toMatch(/sign up/i);
    expect(await voiceOver.lastSpokenPhrase()).toMatch(/button/i);
  } finally {
    await site.close();
  }
});
