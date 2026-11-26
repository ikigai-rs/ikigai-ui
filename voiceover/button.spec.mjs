// Real VoiceOver reaches the gallery's "Sign up" button and says its name and role: the first
// HEAR check of `button`'s html criterion (assets/criterion/button.json), "WHEN I Tab to the
// button with a desktop screen reader ... I HEAR its purpose is clear and I HEAR it identify its
// role of button". Ledger #4278, from the #3545 spike (docs/research/screen-reader-ci.md).
//
// The site is the one the axe job checks (`cargo run --example axe_site`), served by
// axe/serve.mjs with the door's CSP. Assertions are loose on purpose: name and role by pattern,
// never VoiceOver's exact wording, which changes with macOS and browser versions. Each test writes
// what VoiceOver said to results/, which the job uploads as the `voiceover` artifact.
import { voiceOverTest as test } from "@guidepup/playwright";
import { expect } from "@playwright/test";
import { mkdir, writeFile } from "node:fs/promises";
import { serve } from "../axe/serve.mjs";

const SITE = process.env.SITE ?? "../target/axe-site";
const STEPS = 80;
const signUpButton = (text) => /sign up/i.test(text) && /button/i.test(text);

async function keepLog(voiceOver, name) {
  await mkdir("results", { recursive: true });
  const log = await voiceOver.spokenPhraseLog();
  await writeFile(`results/${name}.json`, JSON.stringify(log, null, 2));
}

async function openGallery(page, voiceOver) {
  const site = await serve(SITE);
  await page.goto(`${site.base}/gallery.html`, { waitUntil: "load" });
  await page.locator("main").waitFor();
  await voiceOver.navigateToWebContent();
  return site;
}

// The criterion's own words: Tab to the button, VoiceOver following keyboard focus.
test("by Tab, VoiceOver says the Sign up button's name and role", async ({ page, voiceOver }) => {
  const site = await openGallery(page, voiceOver);
  try {
    let found = false;
    for (let i = 0; i < STEPS && !found; i++) {
      await voiceOver.press("Tab");
      found = signUpButton((await voiceOver.lastSpokenPhrase()) ?? "");
    }
    await keepLog(voiceOver, "spoken-tab");
    expect(found, `Tab never reached a phrase with "Sign up" and "button" in ${STEPS} presses`).toBe(true);
  } finally {
    await site.close();
  }
});

// The spike's route (20 of 20 on macos-15): VoiceOver's own cursor, VO+Right.
test("by VoiceOver's cursor, it says the Sign up button's name and role", async ({ page, voiceOver }) => {
  const site = await openGallery(page, voiceOver);
  try {
    let found = false;
    for (let i = 0; i < STEPS && !found; i++) {
      await voiceOver.next();
      found = signUpButton((await voiceOver.itemText()) ?? "");
    }
    await keepLog(voiceOver, "spoken-cursor");
    expect(found, `no item matched "Sign up" + "button" in ${STEPS} steps`).toBe(true);
    expect(await voiceOver.lastSpokenPhrase()).toMatch(/sign up/i);
    expect(await voiceOver.lastSpokenPhrase()).toMatch(/button/i);
  } finally {
    await site.close();
  }
});
