// Real VoiceOver hears the workspace's two status sentences (ledger #3953, docs/design/workspace.md
// section 5A and section 7 proposal 6): "Contacts maximized. Restore to see every pane." when a
// pane is maximized, and "Closed Table. Revert to version {n} brings it back." when a pane is
// closed. Both are said in #status, the page's one polite live region; neither is focused, so
// VoiceOver says them only as live-region announcements, which this checks.
//
// Over the demo door (demo-door.mjs starts it as globalSetup; DEMO_BASE names it), since close
// needs its edit Sink (a workspace read takes seconds in the demo's debug build). Each test reverts the workspace to version 1 first and after. Assertions
// are loose on purpose, as in button.spec.mjs: the sentence by pattern, anywhere in what
// VoiceOver said after the action, never its exact wording around it. What VoiceOver said is
// written to results/, which the job uploads as the `voiceover` artifact.
import { voiceOverTest as test } from "@guidepup/playwright";
import { expect } from "@playwright/test";
import { mkdir, writeFile } from "node:fs/promises";

const WORKSPACE = "urn:iki:ui:workspace:person:urn:person:demo:name:contacts";
const PATH = "/workspace/" + WORKSPACE;
const enc = (iri) => "/resolution/iri/" + encodeURIComponent(iri);
const WAIT = 45000;
const STEPS = 300;

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

// Whether VoiceOver said something matching `pattern` within WAIT ms. A live-region
// announcement is not a Guidepup command, so it never enters spokenPhraseLog: VoiceOver's last
// phrase is read directly, every 100 ms, and each distinct one is kept in `said` (written to
// results/ with the log).
async function heard(voiceOver, pattern, said) {
  const until = Date.now() + WAIT;
  while (Date.now() < until) {
    const last = (await voiceOver.lastSpokenPhrase()) ?? "";
    if (last && said[said.length - 1] !== last) said.push(last);
    if (pattern.test(last)) return true;
    await new Promise((r) => setTimeout(r, 100));
  }
  return false;
}

async function keepSaid(name, said) {
  await mkdir("results", { recursive: true });
  await writeFile(`results/${name}-last-phrases.json`, JSON.stringify(said, null, 2));
}

async function openWorkspace(page, voiceOver) {
  await page.goto(door() + PATH, { waitUntil: "networkidle" });
  await page.locator("#pane-list > h2[data-ui-handle]").waitFor();
  await voiceOver.navigateToWebContent();
}

test.afterAll(async ({ request }) => {
  if (process.env.DEMO_BASE) await revert(request);
});

// Tab with VoiceOver (keys it sends, so it follows focus, as in button.spec.mjs) until it says
// a phrase `match` accepts, at most STEPS presses. Playwright's own focus and key events are
// synthesized in the page and VoiceOver says nothing for them (the first run, ledger #3953).
async function tabTo(voiceOver, match, what) {
  for (let i = 0; i < STEPS; i++) {
    await voiceOver.press("Tab");
    if (match((await voiceOver.lastSpokenPhrase()) ?? "")) return;
  }
  throw new Error(`Tab never reached ${what} in ${STEPS} presses`);
}

test("maximize: VoiceOver hears the maximize sentence", async ({ page, voiceOver, request }) => {
  test.setTimeout(8 * 60 * 1000);
  await revert(request);
  await openWorkspace(page, voiceOver);
  await tabTo(voiceOver, (t) => /maximize contacts/i.test(t), "Maximize Contacts");
  const said = [];
  await Promise.all([page.waitForURL(/maximize=list/), voiceOver.press("Enter")]);
  const found = await heard(voiceOver, /contacts maximized\.? restore to see every pane/i, said);
  await keepSaid("spoken-maximize", said);
  await keepLog(voiceOver, "spoken-maximize");
  await expect(page.locator("#status p")).toHaveText("Contacts maximized. Restore to see every pane.");
  expect(found, "VoiceOver never said the maximize sentence").toBe(true);
});

test("close: VoiceOver hears the Closed sentence", async ({ page, voiceOver, request }) => {
  test.setTimeout(8 * 60 * 1000);
  await revert(request);
  await openWorkspace(page, voiceOver);
  // Table's menu is opened from the page: Safari's Tab reaches form controls, not a <summary>,
  // and what is checked here is the sentence after Close, which VoiceOver's own keys press.
  const menu = page.locator("#pane-table > details[data-ui-pane-menu]");
  await menu.evaluate((d) => { d.open = true; });
  await expect(menu).toHaveAttribute("open", "");
  try {
    await tabTo(voiceOver, (t) => /\bclose\b/i.test(t) && /button/i.test(t), "Close in Table's menu");
  } finally {
    await keepLog(voiceOver, "spoken-close-tab");
  }
  const said = [];
  await voiceOver.press("Enter");
  const found = await heard(voiceOver, /closed table\.? revert to version \d+ brings it back/i, said);
  await keepSaid("spoken-close", said);
  await keepLog(voiceOver, "spoken-close");
  await expect(page.locator("#pane-table")).toHaveCount(0, { timeout: 30000 });
  await expect(page.locator("#status p")).toHaveText(/^Closed Table\. Revert to version \d+ brings it back\.$/, { timeout: 30000 });
  expect(found, "VoiceOver never said the Closed sentence").toBe(true);
});
