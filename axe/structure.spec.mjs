// Collapse, maximize, close and add in a browser (ledger #3953, docs/design/workspace.md
// section 5A "Collapse, maximize and restore" and "Close and add", section 7 proposal 6), over
// the demo door's workspace and its edit Sink: `DEMO_BASE=<url> npx playwright test -c
// axe/workspace.config.mjs`. scripts/screenshots.sh runs it with the other workspace specs.
//
// 1. Collapse by keyboard: "Collapse Contacts" (aria-expanded="true") sends one edit; the pane
//    keeps its heading and loses its content; the toggle in the same place is "Expand Contacts"
//    (aria-expanded="false") and has focus; #status says "Collapsed Contacts."; and Expand, by a
//    single pointer click, gives it back. Axe and screenshots collapsed in light, dark and more
//    contrast.
// 2. Maximize by a single pointer click: a GET (no edit sent); only Contacts is in the page, so the
//    other panes are out of the tab order and the accessibility tree; the "Panes" list still
//    lists them, "hidden while Contacts is maximized"; the toggle is "Restore every pane"
//    (aria-pressed="true"); #status says "Contacts maximized. Restore to see every pane." Restore by
//    keyboard brings every pane back. Axe and screenshots maximized.
// 3. Add by keyboard from the "Add pane" form, then close: the new pane's heading has focus and
//    #status says "Opened Contact table below Table."; its menu's Close sends one edit, focus is
//    on the heading of the pane that took its place (Table), and #status says "Closed Contact
//    table. Revert to version {n} brings it back." Axe and screenshots of the add form.
import { test, expect } from "@playwright/test";
import { AxeBuilder } from "@axe-core/playwright";

const base = process.env.DEMO_BASE;
if (!base) throw new Error("set DEMO_BASE to the demo door's URL (scripts/screenshots.sh does)");
const WORKSPACE = "urn:iki:ui:workspace:person:urn:person:demo:name:contacts";
const PATH = "/workspace/" + WORKSPACE;
const enc = (iri) => "/resolution/iri/" + encodeURIComponent(iri);
const TAGS = ["wcag2a", "wcag2aa", "wcag21a", "wcag21aa", "wcag22aa"];
const MODES = [
  { name: "light", media: { colorScheme: "light", contrast: "no-preference", forcedColors: "none" } },
  { name: "dark", media: { colorScheme: "dark", contrast: "no-preference", forcedColors: "none" } },
  { name: "more-contrast", media: { colorScheme: "light", contrast: "more", forcedColors: "none" } },
];
const SHOTS = process.env.STRUCTURE_SHOTS_DIR || "../target/screenshots/structure";

async function revert(request) {
  const r = await request.post(base + enc(WORKSPACE + ":revert"), { form: { "to-version": "1" } });
  expect(r.ok(), await r.text()).toBe(true);
}

// The latest version's layout, as the store answers it.
async function latest(request) {
  const r = await request.get(base + enc(WORKSPACE), { headers: { accept: "application/json" } });
  expect(r.ok()).toBe(true);
  return (await r.json()).layout;
}

async function open(page, width = 1000) {
  await page.setViewportSize({ width, height: 900 });
  const errors = [];
  page.on("pageerror", (e) => errors.push(e.message));
  page.on("console", (m) => {
    if (m.type() === "error") errors.push(m.text());
  });
  const edits = [];
  page.on("request", (r) => {
    if (r.method() === "POST" && decodeURIComponent(r.url()).includes(WORKSPACE + ":edit")) edits.push(r.postData());
  });
  await page.goto(base + PATH, { waitUntil: "networkidle" });
  await page.locator("[data-ui-tabs][data-ui-tabs-ready]").waitFor();
  await page.locator("#pane-list > h2[data-ui-handle]").waitFor();
  return { errors, edits };
}

// The panes in DOM order, and the "Panes" list's order.
async function orders(page) {
  return page.evaluate(() => ({
    dom: [...document.querySelectorAll("section.ui-pane[id^='pane-']")].map((s) => s.id),
    listed: [...document.querySelectorAll('nav[aria-label="Panes"] a')].map((a) => a.hash.slice(1)),
  }));
}

// The middle of the part of a box on screen (900 px tall).
function mid(box) {
  return (Math.max(box.y, 0) + Math.min(box.y + box.height, 900)) / 2;
}

async function status(page) {
  return page.evaluate(() => [...document.querySelectorAll("#status p")].map((p) => p.textContent));
}

async function axe(page, what) {
  for (const mode of MODES) {
    await page.emulateMedia(mode.media);
    const result = await new AxeBuilder({ page }).withTags(TAGS).analyze();
    const found = result.violations.map((v) => `${v.id}: ${v.nodes.map((n) => n.target.join(" ")).join(", ")}`);
    expect(found, `axe ${what}, ${mode.name}`).toEqual([]);
  }
  await page.emulateMedia(MODES[0].media);
}

// An edit is saved, then the page's workspace read again (the demo's debug build composes
// every pane's lens for it, which takes seconds on a CI runner).
async function waitSettled(page) {
  await expect(page.locator("#status p")).toHaveCount(1, { timeout: 30000 });
}


async function shoot(page, what) {
  for (const mode of MODES) {
    await page.emulateMedia(mode.media);
    await page.screenshot({ path: `${SHOTS}/${what}-${mode.name}.png`, fullPage: true });
  }
  await page.emulateMedia(MODES[0].media);
}

test.setTimeout(180000);
test.beforeEach(async ({ request }) => revert(request));
test.afterAll(async ({ request }) => revert(request));

test("collapse by keyboard and expand by pointer: aria-expanded, focus, one sentence", async ({ page, request }) => {
  const { errors, edits } = await open(page);
  const collapse = page.locator("#pane-list > form.ui-pane-toggle > button[aria-expanded]");
  await expect(collapse).toHaveAccessibleName("Collapse Contacts");
  await expect(collapse).toHaveAttribute("aria-expanded", "true");
  await collapse.focus();
  await page.keyboard.press("Enter");
  await waitSettled(page);
  expect(edits.length, "one edit").toBe(1);
  expect(await status(page)).toEqual(["Collapsed Contacts."]);
  await expect(page.locator("#pane-list")).toHaveAttribute("data-ui-collapsed", "true");
  await expect(page.locator("#pane-list > h2")).toHaveText("Contacts");
  await expect(page.locator("#pane-list > .ui-group")).toHaveCount(0);
  const expand = page.locator("#pane-list > form.ui-pane-toggle > button[aria-expanded]");
  await expect(expand).toHaveAccessibleName("Expand Contacts");
  await expect(expand).toHaveAttribute("aria-expanded", "false");
  await expect(expand).toBeFocused();
  expect((await latest(request)).children[0].collapsed).toBe(true);
  await shoot(page, "collapsed");
  await axe(page, "collapsed");
  await expand.click();
  await expect(page.locator("#pane-list")).toHaveAttribute("data-ui-collapsed", "false", { timeout: 30000 });
  expect(edits.length, "two edits").toBe(2);
  await expect(page.locator("#pane-list > form.ui-pane-toggle > button[aria-expanded]")).toHaveAttribute("aria-expanded", "true");
  expect(errors).toEqual([]);
});

test("maximize by pointer and restore by keyboard: a view, aria-pressed, the pane list", async ({ page }) => {
  const { errors, edits } = await open(page);
  const max = page.locator("#pane-list > form.ui-pane-toggle > button[aria-pressed]");
  await expect(max).toHaveAccessibleName("Maximize Contacts");
  await expect(max).toHaveAttribute("aria-pressed", "false");
  await Promise.all([page.waitForURL(/maximize=list/), max.click()]);
  await page.waitForLoadState("networkidle");
  expect(edits.length, "maximize writes nothing").toBe(0);
  await expect(page.locator("#status p")).toHaveText("Contacts maximized. Restore to see every pane.");
  await expect(page.locator("section.ui-pane[id^='pane-']")).toHaveCount(1);
  await expect(page.locator("#pane-table, #pane-dashboard")).toHaveCount(0);
  const listed = await page.locator('nav[aria-label="Panes"] a').allTextContents();
  expect(listed.filter((t) => t.endsWith("hidden while Contacts is maximized")).length).toBe(2);
  // Nothing of another pane is in the tab order.
  const reached = [];
  await page.locator("h1").first().focus();
  for (let i = 0; i < 60; i++) {
    await page.keyboard.press("Tab");
    const at = await page.evaluate(() => {
      const p = document.activeElement && document.activeElement.closest("section.ui-pane");
      return p ? p.id : null;
    });
    if (at) reached.push(at);
  }
  expect([...new Set(reached)]).toEqual(["pane-list"]);
  const restore = page.locator("#pane-list > form.ui-pane-toggle > button[aria-pressed]");
  await expect(restore).toHaveAccessibleName("Restore every pane");
  await expect(restore).toHaveAttribute("aria-pressed", "true");
  await shoot(page, "maximized");
  await axe(page, "maximized");
  await restore.focus();
  await Promise.all([page.waitForURL((u) => !u.search.includes("maximize")), page.keyboard.press("Enter")]);
  await page.waitForLoadState("networkidle");
  await expect(page.locator("section.ui-pane[id^='pane-']")).toHaveCount(3);
  expect(page.url()).toContain("#pane-list");
  expect(edits.length).toBe(0);
  expect(errors).toEqual([]);
});

test("add by keyboard, then close: focus on the new pane, then on the one that took its place", async ({ page, request }) => {
  const { errors, edits } = await open(page);
  const form = page.locator("form.ui-form", { has: page.locator('input[name="op"][value="add"]') }).first();
  await expect(form.getByRole("heading", { name: "Add pane" })).toBeVisible();
  await form.getByLabel("Open").selectOption({ label: "Contact table" });
  await form.getByLabel("Beside").selectOption({ label: "Table" });
  await form.getByLabel("Where").selectOption({ label: "Below it" });
  await shoot(page, "add-form");
  await axe(page, "the add form");
  await form.getByRole("button", { name: "Add pane" }).focus();
  await page.keyboard.press("Enter");
  await waitSettled(page);
  expect(edits.length, "one edit").toBe(1);
  expect(await status(page)).toEqual(["Opened Contact table below Table."]);
  await expect(page.locator("#pane-contact-table > h2")).toBeFocused();
  const summary = page.locator("#pane-contact-table > details[data-ui-pane-menu] > summary");
  await summary.focus();
  await page.keyboard.press("Enter");
  const close = page.locator("#pane-contact-table > details[data-ui-pane-menu]").getByRole("button", { name: "Close" });
  await close.focus();
  await page.keyboard.press("Enter");
  await expect(page.locator("#pane-contact-table")).toHaveCount(0, { timeout: 30000 });
  await waitSettled(page);
  expect(edits.length, "two edits").toBe(2);
  expect((await status(page))[0]).toMatch(/^Closed Contact table\. Revert to version \d+ brings it back\.$/);
  await expect(page.locator("#pane-table > h2")).toBeFocused();
  expect(await page.evaluate(() => document.activeElement !== document.body)).toBe(true);
  const { dom, listed } = await orders(page);
  expect(dom).toEqual(["pane-list", "pane-dashboard", "pane-table"]);
  expect(listed).toEqual(dom);
  await axe(page, "after a close");
  expect(errors).toEqual([]);
});
