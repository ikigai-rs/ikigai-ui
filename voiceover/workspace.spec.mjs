// Real VoiceOver hears the workspace's two status sentences (ledger #3953, docs/design/workspace.md
// section 5A and section 7 proposal 6): "Contacts maximized. Restore to see every pane." when a
// pane is maximized, and "Closed Dashboard. Revert to version {n} brings it back." when a pane is
// closed. The Closed sentence is said in #status, the page's one polite live region, as a
// live-region announcement. The maximize sentence is rendered with the page as #ui-said and
// describes the Restore toggle, which the page focuses on arrival, so VoiceOver says Restore's
// name and then the sentence as its description, which this checks in that order.
//
// Over the demo door (demo-door.mjs starts it as globalSetup; DEMO_BASE names it), since close
// needs its edit Sink (a workspace read takes seconds in the demo's debug build). Each test reverts the workspace to version 1 first and after. Assertions
// are loose on purpose, as in button.spec.mjs: the sentence by pattern, anywhere in what
// VoiceOver said after the action, never its exact wording around it. What VoiceOver said is
// written to results/, which the job uploads as the `voiceover` artifact.
import { voiceOverTest as test } from "@guidepup/playwright";
import { expect } from "@playwright/test";
import { mkdir, writeFile } from "node:fs/promises";
import { execFile } from "node:child_process";

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

// VoiceOver's last phrase, asked of VoiceOver itself (the AppleScript Guidepup runs for its own
// log). Guidepup 0.40's `voiceOver.lastSpokenPhrase()` is NOT that: it returns the last entry of
// Guidepup's log, filled only after a Guidepup command, so polling it after Enter re-read the
// same cached "" for the whole wait and never saw a live-region announcement (run 46000455135,
// ledger #3953: both `said` lists were empty).
const LAST_PHRASE = 'tell application "VoiceOver"\nwith transaction\nreturn content of last phrase\nend transaction\nend tell';
function livePhrase() {
  return new Promise((resolve) => {
    const child = execFile("/usr/bin/osascript", [], { timeout: 5000 }, (error, stdout) => {
      resolve(error ? "" : String(stdout).trim().replace(/^missing value$/, "").replace(/\s\s+/g, " "));
    });
    child.stdin.end(LAST_PHRASE);
  });
}

// Whether VoiceOver said something matching `pattern` within WAIT ms: its live last phrase is
// read every 100 ms until the phrase comes (never a fixed sleep), and each distinct one is kept
// in `said` (written to results/ with the log).
async function heard(voiceOver, from, pattern, said) {
  // What Guidepup itself captured while its Enter command waited counts too.
  const logged = (await voiceOver.spokenPhraseLog()).slice(from);
  said.push(...logged.filter(Boolean));
  if (logged.some((t) => pattern.test(t))) return true;
  const until = Date.now() + WAIT;
  while (Date.now() < until) {
    const last = await livePhrase();
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
  const from = (await voiceOver.spokenPhraseLog()).length;
  await Promise.all([page.waitForURL(/maximize=list/), voiceOver.press("Enter")]);
  // Focus lands on Restore, described by the sentence the page renders at load (#ui-said): VoiceOver
  // says its name, then the sentence as its description, in that order.
  const found = await heard(voiceOver, from, /contacts maximized\.? restore to see every pane/i, said);
  await keepSaid("spoken-maximize", said);
  await keepLog(voiceOver, "spoken-maximize");
  await expect(page.locator("main #ui-said")).toHaveText("Contacts maximized. Restore to see every pane.");
  expect(found, "VoiceOver never said the maximize sentence").toBe(true);
  const all = said.join(" ").toLowerCase();
  const name = all.indexOf("restore every pane");
  const sentence = all.search(/contacts maximized\.? restore to see every pane/);
  expect(name >= 0 && name < sentence, `VoiceOver said Restore's name before the sentence: ${JSON.stringify(said)}`).toBe(true);
});

test("close: VoiceOver hears the Closed sentence", async ({ page, voiceOver, request }) => {
  test.setTimeout(8 * 60 * 1000);
  await revert(request);
  await openWorkspace(page, voiceOver);
  // Dashboard's menu is opened from the page: Safari's Tab reaches form controls, not a <summary>,
  // and what is checked here is the sentence after Close, which VoiceOver's own keys press.
  const menu = page.locator("#pane-dashboard > details[data-ui-pane-menu]");
  await menu.evaluate((d) => { d.open = true; });
  await expect(menu).toHaveAttribute("open", "");
  try {
    await tabTo(voiceOver, (t) => /\bclose\b/i.test(t) && /button/i.test(t), "Close in Dashboard's menu");
  } finally {
    await keepLog(voiceOver, "spoken-close-tab");
  }
  const said = [];
  const from = (await voiceOver.spokenPhraseLog()).length;
  await voiceOver.press("Enter");
  const found = await heard(voiceOver, from, /closed dashboard\.? revert to version \d+ brings it back/i, said);
  await keepSaid("spoken-close", said);
  await keepLog(voiceOver, "spoken-close");
  await expect(page.locator("#pane-dashboard")).toHaveCount(0, { timeout: 30000 });
  await expect(page.locator("#status p")).toHaveText(/^Closed Dashboard\. Revert to version \d+ brings it back\.$/, { timeout: 30000 });
  expect(found, "VoiceOver never said the Closed sentence").toBe(true);
});
