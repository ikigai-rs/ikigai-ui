# Research spike: can CI hear anything? Real screen readers in CI

Ledger #3545, part of the accessibility strategy. Status: **research note, proof done.** It
changes no CI in this or any repository: the proof ran on a throwaway branch with its own
workflow, and nothing on `main` runs it.

**Verdict: feasible, with caveats.** Real VoiceOver, driven by Guidepup on a GitHub-hosted
`macos-15` runner, reached the gallery's "Sign up" button and said its name and role in 20 runs
of 20, with no retries. It should become a CI job only as an **informational, non-blocking**
one, and only beside the deterministic accessibility-tree check that already blocks
(`axe/aria.spec.mjs`, ledger #3550). Follow-up items are [at the end](#7-proposed-follow-up-items).

What this spike checked, and how:

- **Ran, on GitHub-hosted runners:** branch `spike/voiceover-proof` (head `40dbb54`, 4 commits on
  top of an older `main`), workflow `.github/workflows/voiceover-spike.yml`, run
  [43270631490](https://github.com/ikigai-rs/ikigai-ui/actions/runs/43270631490). Three earlier
  runs on that branch failed on setup (section 4).
- **Read for this note** (2026-11-21): the run's 21 jobs and their timings, all 20 artifacts
  (`spoken.json`, Playwright's `report.json`) and the failed runs' logs, through `gh`. Guidepup's
  repositories, licenses, releases, open issues and its own CI runs, through `gh api`. The W3C AT
  Driver and ARIA-AT repositories' state, not their content.
- **Not done:** no NVDA run (Windows), no Orca run (Linux), no run on a developer's Mac (driving
  VoiceOver takes over the machine's speech and needs privacy grants, which an unattended worker
  must not give itself), no second browser, no second OS image.

## 1. The check

The first HEAR check of `button`'s html criterion (`assets/criterion/button.json`) is "WHEN I Tab
to the button with a desktop screen reader (NVDA, JAWS, VoiceOver) I HEAR its purpose is clear
and I HEAR it identify its role of button." The proof held its name-and-role half against
VoiceOver itself, with one difference from the sentence: it reached the button with VoiceOver's
own cursor (VO+Right), not with Tab. A Tab variant (`voiceOver.press("Tab")`, or Playwright's
keyboard while VoiceOver follows focus) is a small change, not yet run.

```js
// spike/voiceover/button.spec.mjs (branch spike/voiceover-proof), abridged
import { voiceOverTest as test } from "@guidepup/playwright";
test("VoiceOver announces the Sign up button's name and role", async ({ page, voiceOver }) => {
  const site = await serve(SITE);                 // axe/serve.mjs, the door's CSP
  await page.goto(`${site.base}/gallery.html`);
  await voiceOver.navigateToWebContent();
  for (let i = 0; i < 80 && !found; i++) {        // VO+Right, at most 80 steps
    await voiceOver.next();
    const item = (await voiceOver.itemText()) ?? "";
    found = /sign up/i.test(item) && /button/i.test(item);
  }
  expect(found).toBe(true);
  expect(await voiceOver.lastSpokenPhrase()).toMatch(/sign up/i);
  expect(await voiceOver.lastSpokenPhrase()).toMatch(/button/i);
});
```

The site is the one the axe job already checks (`cargo run --example axe_site`), built once on
Linux and passed to the macOS jobs as one tar file. The browser is WebKit (Safari's engine)
headed, as Guidepup's Playwright fixture expects.

## 2. What it measured

| Measure | Result |
|---|---|
| Runs | 20 jobs, `macos-15`, matrix with `max-parallel: 5`, `retries: 0` |
| Passed | **20 of 20** |
| Spoken log | 25 phrases in every run; the last one is `Sign up button` in every run |
| Run-to-run difference | The first phrase: `link Skip to content` in 18 runs, `In About web content Guidepup Marker…` in 2. Where VoiceOver starts is not fixed; the rest of the log was the same. |
| Test time | 21.5 s fastest, 31.6 s median, 38.4 s slowest (Playwright's own duration) |
| Job time | about 1.3 to 2 minutes per job, setup included |
| Whole run | 7 min 53 s wall clock, including the Linux site build |
| Billable minutes | 0 (public repository) |

The log of one run, from `banner` to the button, in VoiceOver's words:

```
banner · Site navigation · link Demo · link About · end of Site navigation · Theme ·
Auto (system) Theme collapsed pop up button · end of banner · main · heading level 1 Gallery ·
Gallery region · Text region · heading level 2 Text · heading level 2 Quarterly report ·
Three items need review. <b> stays text. · Saved. · end of Text region · Input region ·
heading level 2 Input · Sign up form · heading level 3 Sign up · Name · Name edit text ·
Sign up button
```

That log is itself evidence the tree check cannot give: the skip link, the landmarks, the
theme chooser's native select ("collapsed pop up button"), the regions named by their sections,
the form named by its heading and a markup-looking string spoken as text, all as a real screen
reader says them.

## 3. What it can and cannot assert

**Can:**

- That VoiceOver reaches an element by its own navigation (VO+Right, the virtual cursor), and
  what it says there: name, role and state words, by pattern.
- The reading order VoiceOver walks, and which landmarks and regions it announces, from the
  spoken phrase log.
- Differences between what the browser's tree says and what a reader says, which is the only
  thing a screen-reader job adds over `axe/aria.spec.mjs`.

**Cannot, or not reliably:**

- **What a person hears.** Guidepup reads phrases *sent to* speech, not audio: an interrupted or
  queued phrase is logged as if it were heard (Guidepup issue 205, open).
- **Live announcements** (`#status`, "Reconnecting…", a refusal said once). Spoken output
  outside Guidepup's capture window can be missed (issue 183, open, a breaking change in
  progress). The rules in `ACCESSIBILITY.md` that matter most for a reader (one live region, said
  once, quiet live views) are exactly the ones least safe to assert this way today.
- **Exact wording.** VoiceOver's phrasing changes with macOS and Safari versions ("heading level
  3", "edit text", "pop up button"). An assertion must match name and role by pattern, never a
  string; a full-log snapshot would break on every runner image update.
- **Other readers.** One reader (VoiceOver), one browser (WebKit), one OS image. NVDA with
  Chromium or Firefox on Windows is the pairing most users have, and was not run.
- **The native face.** GPUI 0.2.2 exposes no accessibility tree, so no reader can hear it; every
  gpui HEAR check stays blocked by the toolkit, whatever runs in CI.
- **Braille output**, and focus behaviour outside what VoiceOver's cursor reports.

## 4. Setup, and what it took

Four pushes, all setup, none of them VoiceOver itself:

| Run | Failed at | Fix |
|---|---|---|
| 43267879875 | the Linux `site` job: `upload-artifact` refuses a file whose path has a colon (the scene datum is stored under an IRI) | tar the site, upload one file |
| 43268385863 | all 20: `VoiceOver cannot be started`, cause `Failed to mount Guidepup preferences` | also run `@guidepup/setup … install`, as Guidepup's own CI does |
| 43269771306 | all 20: `No local installation of '@guidepup/guidepup' was found`, then `Error.isError is not a function` | run it from the folder where Guidepup is installed, on Node 24 |
| 43270631490 | passed, 20 of 20 | |

The working steps, on `macos-15`, each pinned:

1. `actions/setup-node` with Node 24.
2. `npx @guidepup/setup@0.29.3 setup --ci` (enables VoiceOver's AppleScript control and grants
   the automation permissions; on a hosted runner that means writing the privacy (TCC) database,
   which depends on how GitHub's image is configured, not on anything GitHub documents. A future
   image could break it without notice).
3. `npm install` of `@guidepup/guidepup@0.40.0`, `@guidepup/playwright@0.21.0`,
   `@playwright/test@1.64.0` (the same Playwright as `axe/`).
4. `npx @guidepup/setup@0.29.3 install` from that folder (mounts VoiceOver's preferences).
5. `npx playwright install webkit`, then `npx playwright test`, headed, one worker.

`guidepup/setup-action` is archived; the CLI above replaces it.

## 5. Flakiness, cost and licensing

**Flakiness.** 0 failures in 20 here. Guidepup's own VoiceOver and NVDA workflows were green in
their last 30 runs each when read for this note (28 success, 2 awaiting approval, which is a
fork-PR gate, not a failure); an earlier reading of the same workflow had found 6 red in 30, so
the window matters. Twenty runs of one check bound the failure rate loosely (below about 15% at
95% confidence), not tightly. The known sources of flake are the start position (seen here, 2 of
20, harmless only because the test walks forward until it finds the button), speech timing
(issue 183) and runner image updates.

**Cost.** Free on this public repository (billable 0 ms measured). About 2 macOS minutes per
check job and 30 s of test per check; each further check on the same page adds seconds, not
minutes, if they share one VoiceOver session. On a private repository macOS minutes are billed at
several times the Linux rate; a single job per push is still small. Queueing is the real cost:
hosted macOS runners start more slowly than Linux ones (this run's later jobs waited for the
five-at-a-time limit, about 5 minutes end to end for 20).

**Licensing.** Guidepup, its Playwright fixture and its setup CLI are MIT, compatible with this
repository's MIT/Apache-2.0. VoiceOver ships with macOS, which GitHub's hosted image already
licenses. NVDA is GPL-2.0, run as a separate program, not linked. No third-party service is
involved.

## 6. Alternatives

| Option | What it is | Fit now |
|---|---|---|
| Playwright `toMatchAriaSnapshot` | Chromium's accessibility tree, name and role | **Already in place** (`axe/aria.spec.mjs`, ledger #3550): deterministic, Linux, blocking. Not a screen reader. |
| Guidepup virtual screen reader | Simulated reader over the DOM tree | Adds little over the tree snapshots above; not a real reader. |
| Guidepup + NVDA on `windows-2022`/`2025` | Real NVDA, Chromium or Firefox | Same library, the most-used pairing; not run here. Worth one proof. |
| Guidepup + Orca on Linux | Real Orca, new in Guidepup | Too new to rely on; watch. |
| W3C AT Driver + ARIA-AT harness | The standard protocol for driving readers | The path to watch: vendor support is partial and it targets reader conformance, not a site's own checks. Do not adopt yet. |
| Commercial hosted readers | Paid real-reader sessions in the cloud | A third-party service and a key in CI; not needed while the free route works. |

## 7. Should it become a CI job?

**Yes, informational and non-blocking, not as a gate.** The reasons:

- The blocking job should stay the deterministic one (`axe/aria.spec.mjs` on Linux). A real
  reader's job catches only where the reader and the browser disagree, and its wording drifts
  with images it does not control, so a red run would often mean "macOS changed", not "we broke
  something".
- It is cheap and, on this evidence, stable for a name-and-role check.
- Its spoken log is a record the accessibility statement does not have today: `ACCESSIBILITY.md`
  says every html HEAR check (53) is untested. One passing VoiceOver check lets that row say
  "1 checked with VoiceOver on macOS 15 and WebKit; 52 untested", honestly scoped.

Adding a workflow is a maintainer's change: a worker token cannot push workflow files, and this
item adds none.

## 8. Proposed follow-up items

1. **Informational VoiceOver job on macOS.** Add a non-blocking `voiceover` job to ikigai-ui's
   CI from the spike branch's workflow (pinned versions, one job, `continue-on-error`), uploading
   the spoken log as an artifact. Needs a maintainer to push the workflow.
2. **Cover more html HEAR checks with VoiceOver.** Add the button check as its criterion words it
   (by Tab, not VoiceOver's cursor), then extend the test to a check per gallery kind
   whose HEAR sentence names a name and role (link, field, heading, table, list, disclosure,
   meter), in one VoiceOver session, each by pattern.
3. **Record HEAR coverage per reader.** Let a criterion's HEAR check carry which reader, browser
   and OS heard it, so the support matrix and `ACCESSIBILITY.md` can count "checked with
   VoiceOver/WebKit on macOS 15" separately from "untested".
4. **NVDA proof on a Windows runner.** Repeat the 20-run proof with Guidepup and NVDA on
   `windows-2025` with Chromium, the most-used reader and browser pairing, on a throwaway branch.
5. **Live-region announcements, once Guidepup captures all speech.** When Guidepup's
   "capture all spoken output" change (issue 183) is released, try asserting that `#status` is
   said exactly once for a refusal and for "Reconnecting…"; until then, leave those to the tree
   and DOM tests.
6. **Watch W3C AT Driver.** Revisit AT Driver and the ARIA-AT harness when a reader ships a
   stable implementation, as a replacement for a 0.x library's private automation route.
