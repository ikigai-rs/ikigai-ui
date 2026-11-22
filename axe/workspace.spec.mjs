// The workspace page in a browser (ledger #3967, docs/design/workspace.md sections 5 and 5A),
// over the demo door's own workspace: `DEMO_BASE=<url> npx playwright test -c
// axe/workspace.config.mjs`. scripts/screenshots.sh starts the demo door and runs this after the
// screenshots, so CI's `screenshots` job runs it. The page is the real route,
// `/workspace/{iri}` (`Pages::workspaces`), so the "Panes" list's links are same-document links,
// as they are on a host.
//
// The demo's workspace (demo/assets/workspace.json): Contacts (400) beside a tab list (600) of
// Dashboard (active) and Table.
//
// 1. At 320 CSS px (WCAG 2.2 SC 1.4.10 Reflow): no horizontal scroll, every pane in one column
//    in reading order (the "Panes" list's order), no splitter shown, and the tabs kept: a tab
//    list with one panel showing. At 1000 px the same page is side by side, so the check can
//    tell the two apart.
// 2. The tab list's keys (the ARIA tabs pattern, shell.js section 7): one tab in the tab order;
//    Right, Left (both wrapping), Home and End move focus and show that tab's panel, the others
//    hidden; and a "Panes" link into the hidden pane shows its tab, without leaving the page.
import { test, expect } from "@playwright/test";

const base = process.env.DEMO_BASE;
if (!base) throw new Error("set DEMO_BASE to the demo door's URL (scripts/screenshots.sh does)");
const PATH = "/workspace/urn:iki:ui:workspace:person:urn:person:demo:name:contacts";
const ORDER = ["pane-list", "pane-dashboard", "pane-table"];

async function open(page, width) {
  await page.setViewportSize({ width, height: 900 });
  const errors = [];
  page.on("pageerror", (e) => errors.push(e.message));
  page.on("console", (m) => {
    if (m.type() === "error") errors.push(m.text());
  });
  await page.goto(base + PATH, { waitUntil: "networkidle" });
  await page.locator("[data-ui-tabs][data-ui-tabs-ready]").waitFor();
  return errors;
}

// Each pane's box, in the "Panes" list's order, with whether it is shown.
async function panes(page) {
  return page.evaluate((ids) => {
    const listed = [...document.querySelectorAll('nav[aria-label="Panes"] a')].map((a) => a.hash.slice(1));
    const boxes = ids.map((id) => {
      const el = document.getElementById(id);
      const r = el.getBoundingClientRect();
      return { id, shown: r.width > 0 && r.height > 0, left: r.left, right: r.right, top: r.top, bottom: r.bottom };
    });
    return { listed, boxes };
  }, ORDER);
}

test("at 320 CSS px the workspace is one column in reading order, no horizontal scroll, its tabs kept", async ({ page }) => {
  const errors = await open(page, 320);

  const scroll = await page.evaluate(() => ({
    doc: document.documentElement.scrollWidth,
    body: document.body.scrollWidth,
    view: document.documentElement.clientWidth,
  }));
  expect(scroll.view).toBe(320);
  expect(scroll.doc, "no horizontal scroll").toBeLessThanOrEqual(scroll.view);
  expect(scroll.body, "no horizontal scroll").toBeLessThanOrEqual(scroll.view);
  // Nothing in the page reaches past the viewport's right edge.
  const wide = await page.evaluate((w) =>
    [...document.querySelectorAll("main *")]
      .filter((e) => {
        const r = e.getBoundingClientRect();
        return r.width > 0 && r.right > w + 0.5;
      })
      .map((e) => e.tagName + (e.id ? "#" + e.id : "") + (e.className ? "." + e.className : "")),
  320);
  expect(wide, "elements past the right edge").toEqual([]);

  const { listed, boxes } = await panes(page);
  expect(listed).toEqual(ORDER);
  const shown = boxes.filter((b) => b.shown);
  // The tab list's active panel shows; the other tab's pane is hidden until chosen.
  expect(shown.map((b) => b.id)).toEqual(["pane-list", "pane-dashboard"]);
  for (const b of shown) {
    expect(b.left, `${b.id} starts inside the viewport`).toBeGreaterThanOrEqual(0);
    expect(b.right, `${b.id} ends inside the viewport`).toBeLessThanOrEqual(320);
  }
  // One column, in reading order: each shown pane starts below the one before it ends.
  for (let i = 1; i < shown.length; i++) {
    expect(shown[i].top, `${shown[i].id} below ${shown[i - 1].id}`).toBeGreaterThanOrEqual(shown[i - 1].bottom - 0.5);
  }
  // No splitter in the narrow form: the Layout form is the way to size panes there.
  await expect(page.getByRole("separator")).toHaveCount(0);

  // The tabs are kept, and work.
  const tablist = page.getByRole("tablist", { name: "Tabs: Dashboard, Table" });
  await expect(tablist).toBeVisible();
  await expect(tablist.getByRole("tab")).toHaveCount(2);
  await expect(page.getByRole("tabpanel")).toHaveCount(1);
  await tablist.getByRole("tab", { name: "Dashboard" }).focus();
  await page.keyboard.press("ArrowRight");
  await expect(tablist.getByRole("tab", { name: "Table" })).toBeFocused();
  await expect(page.locator("#pane-table")).toBeVisible();
  const after = await page.evaluate(() => document.documentElement.scrollWidth);
  expect(after, "no horizontal scroll with the table showing").toBeLessThanOrEqual(320);

  expect(errors).toEqual([]);
});

test("at 1000 CSS px the same workspace is side by side, with its splitter", async ({ page }) => {
  const errors = await open(page, 1000);
  const { boxes } = await panes(page);
  const [list, dashboard] = boxes;
  expect(dashboard.left, "the tab list is beside Contacts").toBeGreaterThanOrEqual(list.right - 0.5);
  expect(dashboard.top).toBeLessThan(list.bottom);
  await expect(page.getByRole("separator", { name: "Resize Contacts and Dashboard" })).toBeVisible();
  expect(errors).toEqual([]);
});

test("the tab list's arrow keys, Home and End move focus and show the panel; one tab is in the tab order", async ({ page }) => {
  const errors = await open(page, 1000);
  const tablist = page.getByRole("tablist", { name: "Tabs: Dashboard, Table" });
  const dashboard = tablist.getByRole("tab", { name: "Dashboard" });
  const table = tablist.getByRole("tab", { name: "Table" });
  const showing = async (tab, pane, other) => {
    await expect(tab).toBeFocused();
    await expect(tab).toHaveAttribute("aria-selected", "true");
    await expect(tab).toHaveAttribute("tabindex", "0");
    await expect(page.locator(`#${pane}`)).toBeVisible();
    await expect(page.locator(`#${other}`)).toBeHidden();
    // Exactly one tab in the tab order, the selected one.
    await expect(tablist.locator('[role="tab"]:not([tabindex="-1"])')).toHaveCount(1);
  };

  await expect(dashboard).toHaveAttribute("aria-selected", "true");
  await expect(table).toHaveAttribute("tabindex", "-1");
  await expect(page.locator("#pane-table")).toBeHidden();

  // Tab reaches the selected tab, and the next Tab leaves the tab list: the other tab is not a
  // stop of its own.
  await dashboard.focus();
  await page.keyboard.press("Shift+Tab");
  await expect(dashboard).not.toBeFocused();
  await page.keyboard.press("Tab");
  await expect(dashboard).toBeFocused();
  await page.keyboard.press("Tab");
  expect(await page.evaluate(() => document.activeElement.getAttribute("role"))).not.toBe("tab");

  await dashboard.focus();
  await page.keyboard.press("ArrowRight");
  await showing(table, "pane-table", "pane-dashboard");
  await page.keyboard.press("ArrowRight"); // wraps
  await showing(dashboard, "pane-dashboard", "pane-table");
  await page.keyboard.press("ArrowLeft"); // wraps
  await showing(table, "pane-table", "pane-dashboard");
  await page.keyboard.press("Home");
  await showing(dashboard, "pane-dashboard", "pane-table");
  await page.keyboard.press("End");
  await showing(table, "pane-table", "pane-dashboard");
  await page.keyboard.press("Home");
  await showing(dashboard, "pane-dashboard", "pane-table");

  // From the tab order, the selected tab is where Tab lands again after the keys moved it.
  await page.keyboard.press("End");
  await page.keyboard.press("Tab");
  await page.keyboard.press("Shift+Tab");
  await expect(table).toBeFocused();
  expect(errors).toEqual([]);
});

test("a Panes link into a hidden pane shows its tab, on the same page", async ({ page }) => {
  const errors = await open(page, 1000);
  const tablist = page.getByRole("tablist", { name: "Tabs: Dashboard, Table" });
  await expect(page.locator("#pane-table")).toBeHidden();
  // A mark on this document: a navigation to another page would lose it.
  await page.evaluate(() => {
    window.__sameDocument = true;
  });

  await page.locator('nav[aria-label="Panes"] a[href$="#pane-table"]').click();
  await expect(page).toHaveURL(/#pane-table$/);
  await expect(page.locator("#pane-table")).toBeVisible();
  await expect(page.locator("#pane-dashboard")).toBeHidden();
  await expect(tablist.getByRole("tab", { name: "Table" })).toHaveAttribute("aria-selected", "true");
  await expect(tablist.getByRole("tab", { name: "Table" })).toHaveAttribute("tabindex", "0");
  await expect(tablist.getByRole("tab", { name: "Dashboard" })).toHaveAttribute("aria-selected", "false");
  expect(await page.evaluate(() => window.__sameDocument)).toBe(true);

  // And back to a pane in the other tab.
  await page.locator('nav[aria-label="Panes"] a[href$="#pane-dashboard"]').click();
  await expect(page.locator("#pane-dashboard")).toBeVisible();
  await expect(page.locator("#pane-table")).toBeHidden();
  expect(await page.evaluate(() => window.__sameDocument)).toBe(true);
  expect(errors).toEqual([]);
});
