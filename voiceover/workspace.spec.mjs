// Real VoiceOver hears the workspace's two status sentences (ledger #3953, docs/design/workspace.md
// section 5A and section 7 proposal 6): "Contacts maximized. Restore to see every pane." when a
// pane is maximized, and "Closed Table. Revert to version {n} brings it back." when a pane is
// closed. Both are said in #status, the page's one polite live region; neither is focused, so
// VoiceOver says them only as live-region announcements, which this checks.
//
// Over the demo door (demo-door.mjs starts it as globalSetup; DEMO_BASE names it), since close
// needs its edit Sink. Each test reverts the workspace to version 1 first and after. Assertions
// are loose on purpose, as in button.spec.mjs: the sentence by pattern, anywhere in what
// VoiceOver said after the action, never its exact wording around it. What VoiceOver said is
// written to results/, which the job uploads as the `voiceover` artifact.
import { voiceOverTest as test } from "@guidepup/playwright";
import { expect } from "@playwright/test";
import { mkdir, writeFile } from "node:fs/promises";

const WORKSPACE = "urn:iki:ui:workspace:person:urn:person:demo:name:contacts";
const PATH = "/workspace/" + WORKSPACE;
const enc = (iri) => "/resolution/iri/" + encodeURIComponent(iri);
const WAIT = 20000;

function door() {
  const base = process.env.DEMO_BASE;
  if (!base) throw new Error(`no demo door: ${process.env.DEMO_FAILED ?? "DEMO_BASE is not set"}`);
  return base;
}

async function revert(request) {
  const r = await request.post(door() + enc(WORKSPACE + ":revert"), { form: { "to-version": "1" } });
  expect(r.ok(), await r.text()).toBe(true);
}

async function keepLog(voiceOver, name) {
  await mkdir("results", { recursive: true });
  const log = await voiceOver.spokenPhraseLog();
  await writeFile(`results/${name}.json`, JSON.stringify(log, null, 2));
}

// Whether VoiceOver said something matching `pattern` after the first `from` phrases of its log,
// within WAIT ms.
async function heard(voiceOver, from, pattern) {
  const until = Date.now() + WAIT;
  while (Date.now() < until) {
    const log = await voiceOver.spokenPhraseLog();
    if (log.slice(from).some((p) => pattern.test(p ?? ""))) return true;
    await new Promise((r) => setTimeout(r, 250));
  }
  return false;
}

async function openWorkspace(page, voiceOver) {
  await page.goto(door() + PATH, { waitUntil: "networkidle" });
  await page.locator("#pane-list > h2[data-ui-handle]").waitFor();
  await voiceOver.navigateToWebContent();
}

test.afterAll(async ({ request }) => {
  if (process.env.DEMO_BASE) await revert(request);
});

test("maximize: VoiceOver hears the maximize sentence", async ({ page, voiceOver, request }) => {
  test.setTimeout(5 * 60 * 1000);
  await revert(request);
  await openWorkspace(page, voiceOver);
  const max = page.locator("#pane-list > form.ui-pane-toggle > button[aria-pressed]");
  await max.focus();
  const from = (await voiceOver.spokenPhraseLog()).length;
  await Promise.all([page.waitForURL(/maximize=list/), page.keyboard.press("Enter")]);
  await expect(page.locator("#status p")).toHaveText("Contacts maximized. Restore to see every pane.");
  const found = await heard(voiceOver, from, /contacts maximized\.? restore to see every pane/i);
  await keepLog(voiceOver, "spoken-maximize");
  expect(found, "VoiceOver never said the maximize sentence").toBe(true);
});

test("close: VoiceOver hears the Closed sentence", async ({ page, voiceOver, request }) => {
  test.setTimeout(5 * 60 * 1000);
  await revert(request);
  await openWorkspace(page, voiceOver);
  const summary = page.locator("#pane-table > details[data-ui-pane-menu] > summary");
  await summary.focus();
  await page.keyboard.press("Enter");
  const close = page.locator("#pane-table > details[data-ui-pane-menu]").getByRole("button", { name: "Close" });
  await close.focus();
  const from = (await voiceOver.spokenPhraseLog()).length;
  await page.keyboard.press("Enter");
  await expect(page.locator("#pane-table")).toHaveCount(0, { timeout: 30000 });
  await expect(page.locator("#status p")).toHaveText(/^Closed Table\. Revert to version \d+ brings it back\.$/, { timeout: 30000 });
  const found = await heard(voiceOver, from, /closed table\.? revert to version \d+ brings it back/i);
  await keepLog(voiceOver, "spoken-close");
  expect(found, "VoiceOver never said the Closed sentence").toBe(true);
});
