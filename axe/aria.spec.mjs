// What the browser exposes for each node kind (ledger #3550): its role, its accessible name
// and the state the kind's criterion promises, from Chromium's accessibility tree, before any
// real screen reader runs. A criterion's HEAR check says "I HEAR its name X and its role Y";
// this checks the name and the role Chromium hands to a screen reader, so a screen-reader job
// would only have to catch where a reader and the browser disagree.
//
// The expectations are derived from the per-kind criteria (`assets/criterion/{kind}.json`,
// ledger #3541/#3542): each one below names the kind whose html criterion it checks, and the
// first test holds the criterion's Role sentence to the ARIA role this file expects, so a
// criterion that changes its role fails here until the snapshot follows it. Names are the
// fixtures' labels (the gallery's and the elements' own examples); nothing else of a page's
// text is in a snapshot, and toMatchAriaSnapshot matches a template partially (children in
// order, unlisted ones ignored), so a new paragraph or a reworded hint does not fail it.
//
// Two things Playwright's snapshot does not carry are checked beside it: an accessible
// DESCRIPTION (a field's hint and error) with toHaveAccessibleDescription, and the disclosure's
// expanded state, which Playwright's role table leaves out for a native <summary> (it has no
// ARIA role in HTML-AAM): that is read from Chromium's own tree over CDP.
import { readFileSync } from "node:fs";
import { test, expect } from "@playwright/test";
import { serve } from "./serve.mjs";

const dir = process.env.AXE_SITE;
if (!dir) throw new Error("set AXE_SITE to the site directory (cargo run --example axe_site -- <dir>)");

// The ARIA role each kind's html criterion names, and the words of its Role sentence that say so.
const ROLES = {
  button: ["button", /^Button\./],
  link: ["link", /^Link\./],
  field: ["textbox", /text input/],
  choice: ["combobox, radiogroup", /^Combo box .*radio group/],
  option: ["option, radio", /^Option .*radio button/],
  form: ["form", /^Form\./],
  heading: ["heading", /^Heading, level 1 to 6/],
  text: ["paragraph", /^None \(text\); some screen readers say paragraph/],
  status: ["(none: text)", /^None \(text\)/],
  nav: ["navigation", /^Navigation landmark, and a list/],
  list: ["list", /^List, with its number of items/],
  table: ["table", /^Table, with its number of rows and columns/],
  group: ["region", /^Region \(a named section\)/],
  card: ["region, heading", /^Region \(a named section\), its title a heading/],
  "key-value": ["table", /^Table\./],
  "tag-list": ["list", /^List, with its number of tags/],
  image: ["img", /^Image \(graphic\)/],
  visual: ["figure, img", /^Image \(graphic\), in a figure/],
  disclosure: ["button (summary), expanded", /^Button \(disclosure\)/],
  meter: ["meter", /^Meter, with its value in its range/],
};

function criterion(kind) {
  const url = new URL(`../assets/criterion/${kind}.json`, import.meta.url);
  let sheet;
  try {
    sheet = JSON.parse(readFileSync(url, "utf8"));
  } catch {
    sheet = JSON.parse(readFileSync(new URL(`../assets/element/${kind}.json`, import.meta.url), "utf8"));
  }
  return sheet.criteria.find((c) => c.target === "html");
}

let site;
test.beforeAll(async () => {
  site = await serve(dir);
});
test.afterAll(async () => {
  await site?.close();
});

async function open(page, path) {
  const errors = [];
  page.on("pageerror", (e) => errors.push(e.message));
  await page.goto(site.base + path, { waitUntil: "load" });
  return errors;
}

test("each kind's criterion names the role this file checks", () => {
  for (const [kind, [, words]] of Object.entries(ROLES)) {
    const c = criterion(kind);
    expect(c, `${kind} has an html criterion`).toBeTruthy();
    expect(c.role, `${kind}'s criterion Role`).toMatch(words);
  }
});

test.describe("gallery", () => {
  test.beforeEach(async ({ page }) => {
    expect(await open(page, "/gallery.html")).toEqual([]);
  });

  test("heading and text: a heading with its level; a paragraph, not named", async ({ page }) => {
    await expect(page.getByRole("region", { name: "Text", exact: true })).toMatchAriaSnapshot(`
      - region "Text":
        - heading "Text" [level=2]
        - heading "Quarterly report" [level=2]
        - paragraph
    `);
  });

  test("status kind: plain text in #main, never a live region", async ({ page }) => {
    await expect(page.getByText("Saved.", { exact: true })).toMatchAriaSnapshot(`- paragraph: Saved.`);
    await expect(page.locator("#main").getByRole("status")).toHaveCount(0);
  });

  test("form, field and button: names from their labels, invalid state, hint and error as the description", async ({ page }) => {
    const input = page.getByRole("region", { name: "Input", exact: true });
    await expect(input).toMatchAriaSnapshot(`
      - region "Input":
        - form "Sign up":
          - heading "Sign up" [level=3]
          - textbox "Name"
          - button "Sign up"
        - textbox "Email" [invalid]
        - combobox "Size":
          - option "Small"
          - option "Large" [selected]
        - button "Save"
    `);
    // A field's hint, then its error, read after its label (the field criterion's Group).
    const email = input.getByRole("textbox", { name: "Email", exact: true });
    await expect(email).toHaveAccessibleDescription("Where the reply goes. An email address has an @ in it.");
    await expect(email).toHaveAttribute("aria-invalid", "true");
    await expect(input.getByRole("combobox", { name: "Size", exact: true })).toHaveAccessibleDescription(/^The size to ship\./);
    // A field with no hint and no error has no description, not an empty one read aloud.
    await expect(input.getByRole("textbox", { name: "Name", exact: true })).toHaveAccessibleDescription("");
  });

  test("link and nav: a link by its text; a named navigation landmark with a list of links", async ({ page }) => {
    await expect(page.getByRole("region", { name: "Navigation", exact: true })).toMatchAriaSnapshot(`
      - region "Navigation":
        - link "Help"
        - navigation "Site":
          - list:
            - listitem:
              - link "Home"
            - listitem:
              - link "About"
    `);
  });

  test("list, table and group: a named list, the table's caption and its column and row headers, a named region", async ({ page }) => {
    await expect(page.getByRole("region", { name: "Structure", exact: true })).toMatchAriaSnapshot(`
      - region "Structure":
        - list "Steps":
          - listitem
          - listitem
        - table "Totals":
          - caption: Totals
          - rowgroup:
            - row:
              - columnheader "Fruit"
              - columnheader "Count"
            - row:
              - rowheader "Apples"
              - cell "3"
        - region "Contact"
    `);
  });

  test("image and visual: an image by its alt; a chart's image in a figure named by its caption", async ({ page }) => {
    await expect(page.getByRole("region", { name: "Media", exact: true })).toMatchAriaSnapshot(`
      - region "Media":
        - img "A lighthouse at dusk"
        - figure "Sales rose every month from January to June.":
          - img "Sales by month"
    `);
  });

  test("the shell's status region: one, outside #main, polite", async ({ page }) => {
    // Empty, it is in the tree already (ledger #3565): visually hidden, never display:none, so a
    // reader that announces only a region it knew before the change hears the first sentence.
    const status = page.getByRole("status");
    await expect(status).toHaveCount(1);
    await expect(status).toBeEmpty();
    await expect(status).toMatchAriaSnapshot(`- status`);
    // ...and takes no room on the page while it says nothing.
    const box = await status.boundingBox();
    expect(box.width * box.height).toBeLessThanOrEqual(1);
    await expect(page.locator("#main #status")).toHaveCount(0);
    await expect(status).toHaveAttribute("id", "status");
    await expect(status).toHaveAttribute("aria-live", "polite");
  });
});

test.describe("elements", () => {
  test.beforeEach(async ({ page }) => {
    expect(await open(page, "/elements.html")).toEqual([]);
  });

  test("card, key-value, tag-list and meter: names and roles", async ({ page }) => {
    await expect(page.getByRole("region", { name: "Custom elements", exact: true })).toMatchAriaSnapshot(`
      - region "Custom elements":
        - region "Field notes":
          - heading "Field notes" [level=3]
          - list "Field notes links":
            - listitem:
              - link "Read the survey"
            - listitem:
              - link "Station map"
        - table "Gauge 14":
          - caption: Gauge 14
          - rowgroup:
            - row:
              - rowheader "Station"
              - cell "Mill Lane"
        - list "Topics":
          - listitem:
            - link "hydrology"
        - meter "Reservoir capacity (%)"
    `);
    // The meter's value in its range (the meter criterion's Role), as Chromium exposes it.
    const meter = page.getByRole("meter", { name: "Reservoir capacity (%)", exact: true });
    await expect(meter).toHaveJSProperty("min", 0);
    await expect(meter).toHaveJSProperty("max", 100);
  });

  test("disclosure: a button named by its summary, collapsed, then expanded once opened", async ({ page }) => {
    const details = page.locator("details.ui-disclosure");
    const summary = details.locator("summary");
    const ax = async () => {
      const cdp = await page.context().newCDPSession(page);
      const { nodes } = await cdp.send("Accessibility.getFullAXTree");
      await cdp.detach();
      const node = nodes.find((n) => !n.ignored && n.name?.value === "How often is the gauge read?" && n.role?.value !== "StaticText");
      if (!node) return null;
      const expanded = (node.properties || []).find((p) => p.name === "expanded");
      return { role: node.role.value, expanded: expanded ? expanded.value.value : undefined };
    };
    // Chromium's role for a <summary> is DisclosureTriangle: a screen reader says "button" (or
    // "disclosure triangle") with its expanded state.
    expect(await ax()).toEqual({ role: "DisclosureTriangle", expanded: false });
    await expect(details).toMatchAriaSnapshot(`- group: How often is the gauge read?`);
    await summary.click();
    expect(await ax()).toEqual({ role: "DisclosureTriangle", expanded: true });
    await expect(details).toHaveAttribute("open", "");
  });
});

test.describe("contact", () => {
  test("choice as radios: a radio group named by its legend, each radio by its label, one checked", async ({ page }) => {
    expect(await open(page, "/lens-contact-dashboard-light.html")).toEqual([]);
    await expect(page.locator("#main")).toMatchAriaSnapshot(`
      - region "The contact service at a glance":
        - heading "The contact service at a glance" [level=2]
        - form "Lens and facets":
          - radiogroup "Lens":
            - radio "contact-graph"
            - radio "contact-dashboard" [checked]
          - button "Show"
        - region "At a glance":
          - table "Nodes by type":
            - caption: Nodes by type
            - rowgroup:
              - row:
                - columnheader "Type"
                - columnheader "Count"
    `);
  });

  test("checkboxes: each named by its label, in the form whose label names their group", async ({ page }) => {
    expect(await open(page, "/lens-board-light.html")).toEqual([]);
    await expect(page.getByRole("form", { name: "Lens and facets", exact: true })).toMatchAriaSnapshot(`
      - form "Lens and facets":
        - radiogroup "Lens":
          - radio "demo-board" [checked]
        - 'checkbox "Status: Todo"'
        - 'checkbox "Status: Doing"'
    `);
  });
});

test("the status region says a refusal, naming the control", async ({ page }) => {
  expect(await open(page, "/home-auto.html")).toEqual([]);
  await page.getByRole("button", { name: "Load details", exact: true }).click();
  await expect(page.getByRole("status")).toMatchAriaSnapshot(`
    - status:
      - paragraph: "Load details: not found"
  `);
});
