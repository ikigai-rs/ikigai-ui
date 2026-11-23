// Rearrange and tabs in a browser (ledger #3952, docs/design/workspace.md section 5A
// "Rearrange" and "Accessibility of the interaction"), over the demo door's own workspace and
// its edit Sink (an in-memory store of every version): `DEMO_BASE=<url> npx playwright test -c
// axe/workspace.config.mjs`. scripts/screenshots.sh runs it with workspace.spec.mjs.
//
// The demo's workspace starts as Contacts (400) beside a tab list (600) of Dashboard (active)
// and Table. Each test starts from that layout: it reverts to version 1 first (the revert Sink
// writes it again as a new version), and again after, so workspace.spec.mjs sees it too.
//
// 1. The "Pane" menu, by keyboard alone: "Move…" puts Contacts below Dashboard. One edit is
//    sent; focus is on Contacts' heading; #status says one sentence; the DOM order of the panes
//    and the "Panes" list match the new layout; Contacts' element is the same element (moved,
//    not composed again).
// 2. The same move by pointer, dragging Contacts' heading onto Dashboard's lower edge: the drop
//    target is labelled "Below Dashboard" mid-drag, and the version written is the same atom as
//    the menu's. Axe mid-drag and after, and screenshots mid-drag, in light, dark, more contrast
//    and forced colors.
// 3. A drag let go off any target, or ended with Escape, sends nothing.
// 4. A place the pane's data-ui-drop does not offer (a bound) is not shown and not sent.
// 5. Tab order: Table dragged before Dashboard along the tab list, and the same by "Move tab
//    earlier" from Table's menu by keyboard, write the same atom; focus on Table's heading.
// 6. At 320 CSS px the open "Pane" menu, and the layout after a move, scroll nothing sideways.
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
const SHOTS = process.env.SHOTS_DIR || "../target/screenshots/rearrange";

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

let menuLayout = null;
let tabLayout = null;

test.setTimeout(120000);
test.beforeEach(async ({ request }) => revert(request));
test.afterAll(async ({ request }) => revert(request));

test("the Pane menu moves a pane by keyboard alone: focus, one sentence, DOM order", async ({ page, request }) => {
  const { errors, edits } = await open(page);
  await page.evaluate(() => {
    document.getElementById("pane-list").__same = true;
  });
  const summary = page.locator('#pane-list > details[data-ui-pane-menu] > summary');
  await expect(summary).toHaveAccessibleName("Pane menu: Contacts");
  await summary.focus();
  await page.keyboard.press("Enter");
  await expect(page.locator("#pane-list > details[data-ui-pane-menu]")).toHaveAttribute("open", "");
  // The Move form's choices, by keyboard: Next to Dashboard (first), Where: Below it (fourth).
  const to = page.locator('#pane-list details select[name="to"]').first();
  const place = page.locator('#pane-list details select[name="place"]').first();
  await to.focus();
  await expect(to).toHaveValue("dashboard");
  await place.focus();
  for (let i = 0; i < 3; i++) await page.keyboard.press("ArrowDown");
  await expect(place).toHaveValue("below");
  await page.keyboard.press("Tab");
  await expect(page.locator("#pane-list details button", { hasText: "Move" }).first()).toBeFocused();
  await page.keyboard.press("Enter");
  await waitSettled(page);

  expect(edits.length, "one edit").toBe(1);
  expect(await status(page)).toEqual(["Moved Contacts below Dashboard."]);
  await expect(page.locator("#pane-list > h2")).toBeFocused();
  const { dom, listed } = await orders(page);
  expect(dom).toEqual(["pane-dashboard", "pane-table", "pane-list"]);
  expect(listed).toEqual(dom);
  expect(await page.evaluate(() => document.getElementById("pane-list").__same), "the same element, moved").toBe(true);
  await expect(page.getByRole("group", { name: "Stacked: Dashboard, Contacts" })).toBeVisible();
  menuLayout = await latest(request);
  expect(menuLayout.split).toBe("column");
  await axe(page, "after a move");
  expect(errors).toEqual([]);
});

test("the same move by pointer: a labelled drop target, the same version", async ({ page, request }) => {
  const { errors, edits } = await open(page);
  const handle = await page.locator("#pane-list > h2").boundingBox();
  const target = await page.locator("#pane-dashboard").boundingBox();
  await page.mouse.move(handle.x + 20, handle.y + handle.height / 2);
  await page.mouse.down();
  // Dashboard's lower edge on screen: it is taller than the viewport.
  const bottom = Math.min(target.y + target.height, 900);
  await page.mouse.move(target.x + target.width / 2, bottom - 20, { steps: 8 });
  const drop = page.locator(".ui-drop:not([hidden])");
  await expect(drop).toHaveText("Below Dashboard");
  await expect(page.locator("#pane-list")).toHaveAttribute("data-ui-dragged", "");
  // The drop target in every theme, mid-drag.
  for (const mode of [...MODES, { name: "forced-colors", media: { colorScheme: "light", forcedColors: "active" } }]) {
    await page.emulateMedia(mode.media);
    await page.screenshot({ path: `${SHOTS}/drop-target-${mode.name}.png` });
  }
  await axe(page, "mid-drag");
  await page.mouse.up();
  await waitSettled(page);
  expect(edits.length, "one edit").toBe(1);
  expect(await status(page)).toEqual(["Moved Contacts below Dashboard."]);
  await expect(page.locator("#pane-list > h2")).toBeFocused();
  await expect(page.locator(".ui-drop")).toHaveCount(0);
  const { dom, listed } = await orders(page);
  expect(dom).toEqual(["pane-dashboard", "pane-table", "pane-list"]);
  expect(listed).toEqual(dom);
  expect(await latest(request), "pointer and keyboard write the same layout").toEqual(menuLayout);
  await axe(page, "after a drag");
  expect(errors).toEqual([]);
});

test("a drag let go off any target, or ended with Escape, sends nothing", async ({ page, request }) => {
  const { errors, edits } = await open(page);
  const before = await latest(request);
  const handle = await page.locator("#pane-list > h2").boundingBox();
  const target = await page.locator("#pane-dashboard").boundingBox();
  // Off every pane: the page's heading.
  await page.mouse.move(handle.x + 20, handle.y + handle.height / 2);
  await page.mouse.down();
  const h1 = await page.locator("h1").boundingBox();
  await page.mouse.move(h1.x + 10, h1.y + 5, { steps: 6 });
  await expect(page.locator(".ui-drop:not([hidden])")).toHaveCount(0);
  await page.mouse.up();
  // Over a target, then Escape.
  await page.mouse.move(handle.x + 20, handle.y + handle.height / 2);
  await page.mouse.down();
  await page.mouse.move(target.x + 10, mid(target), { steps: 6 });
  await expect(page.locator(".ui-drop:not([hidden])")).toHaveText("Left of Dashboard");
  await page.keyboard.press("Escape");
  await expect(page.locator(".ui-drop")).toHaveCount(0);
  await page.mouse.up();
  await page.waitForTimeout(300);
  expect(edits).toEqual([]);
  expect(await latest(request)).toEqual(before);
  expect((await orders(page)).dom).toEqual(["pane-list", "pane-dashboard", "pane-table"]);
  expect(errors).toEqual([]);
});

test("a place the pane does not offer (a bound) is not shown and not sent", async ({ page }) => {
  const { errors, edits } = await open(page);
  // As the server writes it for a full tab group: every edge, no tabs.
  await page.evaluate(() => document.getElementById("pane-dashboard").setAttribute("data-ui-drop", "left right above below"));
  const handle = await page.locator("#pane-list > h2").boundingBox();
  const target = await page.locator("#pane-dashboard").boundingBox();
  await page.mouse.move(handle.x + 20, handle.y + handle.height / 2);
  await page.mouse.down();
  await page.mouse.move(target.x + target.width / 2, mid(target), { steps: 8 });
  await expect(page.locator(".ui-drop:not([hidden])")).toHaveCount(0);
  // Its edges still are.
  await page.mouse.move(target.x + target.width - 10, mid(target), { steps: 4 });
  await expect(page.locator(".ui-drop:not([hidden])")).toHaveText("Right of Dashboard");
  await page.mouse.move(target.x + target.width / 2, mid(target), { steps: 4 });
  await page.mouse.up();
  await page.waitForTimeout(300);
  expect(edits).toEqual([]);
  expect(errors).toEqual([]);
});

test("a tab dragged along its tab list, and Move tab earlier by keyboard, write the same order", async ({ page, request }) => {
  let { errors, edits } = await open(page);
  const table = page.getByRole("tab", { name: "Table" });
  const dashboard = await page.getByRole("tab", { name: "Dashboard" }).boundingBox();
  const box = await table.boundingBox();
  await page.mouse.move(box.x + box.width / 2, box.y + box.height / 2);
  await page.mouse.down();
  await page.mouse.move(dashboard.x + 4, dashboard.y + dashboard.height / 2, { steps: 8 });
  await expect(page.locator(".ui-tab-drop:not([hidden])")).toHaveCount(1);
  await page.mouse.up();
  await waitSettled(page);
  expect(edits.length).toBe(1);
  expect(await status(page)).toEqual(["Moved Table to tab 1 of 2."]);
  await expect(page.locator("#pane-table > h2")).toBeFocused();
  await expect(page.getByRole("tablist", { name: "Tabs: Table, Dashboard" })).toBeVisible();
  expect((await orders(page)).dom).toEqual(["pane-list", "pane-table", "pane-dashboard"]);
  tabLayout = await latest(request);

  await revert(request);
  ({ errors, edits } = await open(page));
  await page.getByRole("tab", { name: "Table" }).focus();
  await page.keyboard.press("ArrowRight"); // wraps to Dashboard
  await page.keyboard.press("ArrowLeft"); // back to Table: its panel shows
  await expect(page.locator("#pane-table")).toBeVisible();
  const summary = page.locator("#pane-table > details[data-ui-pane-menu] > summary");
  await summary.focus();
  await page.keyboard.press("Enter");
  const earlier = page.locator("#pane-table details button", { hasText: "Move tab earlier" });
  await earlier.focus();
  await page.keyboard.press("Enter");
  await waitSettled(page);
  expect(edits.length).toBe(1);
  expect(await status(page)).toEqual(["Moved Table to tab 1 of 2."]);
  await expect(page.locator("#pane-table > h2")).toBeFocused();
  expect(await latest(request), "pointer and keyboard write the same tab order").toEqual(tabLayout);
  expect(errors).toEqual([]);
});

test("at 320 CSS px the open Pane menu and the layout after a move scroll nothing sideways", async ({ page }) => {
  const { errors } = await open(page, 320);
  const wide = async () =>
    page.evaluate(() => ({
      doc: document.documentElement.scrollWidth,
      past: [...document.querySelectorAll("main *")]
        .filter((e) => {
          const r = e.getBoundingClientRect();
          return r.width > 0 && r.right > 320.5;
        })
        .map((e) => e.tagName + (e.id ? "#" + e.id : "")),
    }));
  await page.locator("#pane-list > details[data-ui-pane-menu] > summary").click();
  let w = await wide();
  expect(w.doc, "no horizontal scroll with the menu open").toBeLessThanOrEqual(320);
  expect(w.past).toEqual([]);
  await page.locator("#pane-list details button", { hasText: "Move" }).first().click();
  await waitSettled(page);
  w = await wide();
  expect(w.doc, "no horizontal scroll after the move").toBeLessThanOrEqual(320);
  expect(w.past).toEqual([]);
  await axe(page, "at 320 px");
  expect(errors).toEqual([]);
});
