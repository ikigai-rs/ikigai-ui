// Real VoiceOver over the gallery and elements screens, in ONE VoiceOver session: the first html
// HEAR check of each kind whose HEAR sentence names a name and a role (assets/criterion/*.json):
// button, link, field and disclosure reached by Tab as their criteria word it (VoiceOver
// following keyboard focus), heading, list and table by VoiceOver's own find commands
// (VO-Command-H, -X, -T: "move by headings", "reach the list", "reach the table"), and meter by
// VoiceOver's cursor (VO+Right), which has no find command for it. Ledger #4279, extending #4278
// (from the #3545 spike, docs/research/screen-reader-ci.md).
//
// The site is the one the axe job checks (`cargo run --example axe_site`), served by
// axe/serve.mjs with the door's CSP. Assertions are loose on purpose: name and role by pattern,
// never VoiceOver's exact wording, which changes with macOS and browser versions. Every check
// runs even when one before it fails (soft assertions), and each one's result and what VoiceOver
// said for it are written to results/checks.json, beside the whole spoken log in
// results/spoken.json; the job uploads both as the `voiceover` artifact.
import { voiceOverTest as test } from "@guidepup/playwright";
import { expect } from "@playwright/test";
import { mkdir, writeFile } from "node:fs/promises";
import { serve } from "../axe/serve.mjs";

const SITE = process.env.SITE ?? "../target/axe-site";
const TABS = 80; // Tab presses allowed to reach one control
const FINDS = 20; // find commands (or cursor steps, for the meter) allowed to reach one item
const STEPS = 120;

// One check per kind, in the order the route meets them. `reach` says how VoiceOver gets there;
// `find` picks the item by its name; `hear` holds what the HEAR sentence asks to be said.
const GALLERY = [
  {
    kind: "field",
    statement: "WHEN I Tab to the input with a desktop screen reader … I HEAR its purpose is clear and I HEAR it identify itself as its input type",
    reach: "tab",
    find: /\bname\b/i,
    hear: { name: /\bname\b/i, role: /edit text|text field|text input/i },
  },
  {
    kind: "button",
    statement: "WHEN I Tab to the button with a desktop screen reader … I HEAR its purpose is clear and I HEAR it identify its role of button",
    reach: "tab",
    find: /sign up/i,
    hear: { name: /sign up/i, role: /\bbutton\b/i },
  },
  {
    kind: "link",
    statement: "WHEN I Tab to the link with a desktop screen reader … I HEAR its purpose is clear and I HEAR it identify itself as a link",
    reach: "tab",
    find: /\bhelp\b/i,
    hear: { name: /\bhelp\b/i, role: /\blink\b/i },
  },
  {
    kind: "heading",
    statement: "WHEN I move by headings with a desktop screen reader … I HEAR the heading's text and I HEAR it identify itself as a heading at its level",
    reach: "findNextHeading",
    find: /structure/i,
    hear: { name: /structure/i, role: /\bheading\b/i, level: /level\s*2\b/i },
  },
  {
    kind: "list",
    statement: "WHEN I reach the list with a desktop screen reader … I HEAR its name, the role of list and how many items it has",
    reach: "findNextList",
    find: /steps/i,
    hear: { name: /steps/i, role: /\blist\b/i, items: /\b2 items?\b/i },
  },
  {
    kind: "table",
    statement: "WHEN I reach the table with a desktop screen reader … I HEAR its caption and its number of rows and columns",
    reach: "findNextTable",
    find: /totals/i,
    hear: { name: /totals/i, role: /\btable\b/i, rows: /\b\d+ rows?\b/i, columns: /\b\d+ columns?\b/i },
  },
];

const ELEMENTS = [
  {
    kind: "disclosure",
    statement: "WHEN I Tab to the summary with a desktop screen reader … I HEAR its text, a button role and collapsed or expanded",
    reach: "tab",
    find: /how often is the gauge read/i,
    hear: { name: /how often is the gauge read/i, role: /\bbutton\b/i, state: /collapsed|expanded/i },
  },
  {
    kind: "meter",
    statement: "WHEN I reach the meter with a desktop screen reader … I HEAR its label, the role of meter and its value",
    reach: "cursor",
    // VoiceOver calls a meter a "level indicator"; the label is its own item before it, so the
    // meter is found by its role and must say its label itself.
    find: /level indicator|\bmeter\b/i,
    hear: { name: /reservoir capacity/i, role: /level indicator|\bmeter\b/i, value: /\b82\b/ },
  },
];

async function said(voiceOver) {
  const phrase = (await voiceOver.lastSpokenPhrase()) ?? "";
  const item = (await voiceOver.itemText()) ?? "";
  return { phrase, item, text: `${phrase} ${item}` };
}

async function reach(voiceOver, check) {
  const limit = check.reach === "tab" ? TABS : check.reach === "cursor" ? STEPS : FINDS;
  for (let i = 1; i <= limit; i++) {
    if (check.reach === "tab") await voiceOver.press("Tab");
    else if (check.reach === "cursor") await voiceOver.next();
    else await voiceOver.perform(voiceOver.keyboardCommands[check.reach]);
    const heard = await said(voiceOver);
    if (check.find.test(heard.text)) return { ...heard, presses: i };
  }
  return null;
}

async function open(page, voiceOver, base, screen) {
  await page.goto(`${base}/${screen}`, { waitUntil: "load" });
  await page.locator("main").waitFor();
  await voiceOver.navigateToWebContent();
}

test("in one VoiceOver session, each kind's first HEAR check says its name and role", async ({ page, voiceOver }) => {
  test.setTimeout(20 * 60 * 1000);
  const site = await serve(SITE);
  const results = [];
  try {
    for (const [screen, checks] of [["gallery.html", GALLERY], ["elements.html", ELEMENTS]]) {
      await open(page, voiceOver, site.base, screen);
      for (const check of checks) {
        const heard = await reach(voiceOver, check);
        const parts = Object.fromEntries(
          Object.entries(check.hear).map(([part, pattern]) => [part, heard ? pattern.test(heard.text) : false]),
        );
        const pass = heard !== null && Object.values(parts).every(Boolean);
        results.push({
          kind: check.kind,
          screen,
          statement: check.statement,
          reach: check.reach,
          result: pass ? "pass" : "fail",
          reached: heard !== null,
          presses: heard?.presses ?? null,
          heard: heard ? { phrase: heard.phrase, item: heard.item } : null,
          parts,
        });
        expect
          .soft(pass, `${check.kind}: ${heard ? `heard "${heard.text.trim()}", parts ${JSON.stringify(parts)}` : `never reached ${check.find} by ${check.reach}`}`)
          .toBe(true);
      }
    }
  } finally {
    await mkdir("results", { recursive: true });
    await writeFile("results/checks.json", JSON.stringify(results, null, 2));
    await writeFile("results/spoken.json", JSON.stringify(await voiceOver.spokenPhraseLog(), null, 2));
    await site.close();
  }
});
