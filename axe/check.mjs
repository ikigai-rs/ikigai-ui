// axe over a static site (UI conventions D13, D18): `node axe/check.mjs <site-dir>`.
//
// Serves <site-dir> (axe/serve.mjs: its _types.json, and the door default CSP on every
// answer), opens every .html file in it in headless Chromium under three media settings
// (light, dark, and light with prefers-contrast: more), and runs axe with the WCAG 2.0, 2.1
// and 2.2 A and AA rules. It also fails on any page error or console error, so a script
// refused by its integrity value or by the CSP is a failure, not a quiet blank. It exits 1 on
// any violation, and on a site with no pages: a check that covers nothing must not read as a
// pass.
import { chromium } from "playwright";
import { AxeBuilder } from "@axe-core/playwright";
import { pages, serve } from "./serve.mjs";

const TAGS = ["wcag2a", "wcag2aa", "wcag21a", "wcag21aa", "wcag22aa"];
const MODES = [
  { name: "light", media: { colorScheme: "light" } },
  { name: "dark", media: { colorScheme: "dark" } },
  { name: "more-contrast", media: { colorScheme: "light", contrast: "more" } },
];

const dir = process.argv[2];
if (!dir) {
  console.error("usage: node axe/check.mjs <site-dir>");
  process.exit(2);
}
const list = await pages(dir);
if (list.length === 0) {
  console.error(`no .html pages under ${dir}`);
  process.exit(1);
}

const site = await serve(dir);
const browser = await chromium.launch();
let failures = 0;
for (const path of list) {
  for (const mode of MODES) {
    const context = await browser.newContext();
    const page = await context.newPage();
    const errors = [];
    page.on("pageerror", (e) => errors.push(`page error: ${e.message}`));
    page.on("console", (m) => {
      if (m.type() === "error") errors.push(`console error: ${m.text()}`);
    });
    await page.emulateMedia(mode.media);
    await page.goto(site.base + path, { waitUntil: "load" });
    const result = await new AxeBuilder({ page }).withTags(TAGS).analyze();
    const where = `${path} (${mode.name})`;
    for (const v of result.violations) {
      failures += 1;
      console.log(`FAIL ${where}: ${v.id} [${v.impact}] ${v.help}`);
      for (const node of v.nodes) console.log(`    at ${node.target.join(" ")}`);
    }
    for (const e of errors) {
      failures += 1;
      console.log(`FAIL ${where}: ${e}`);
    }
    if (result.violations.length === 0 && errors.length === 0) {
      console.log(`ok   ${where}: ${result.passes.length} rules pass`);
    }
    await context.close();
  }
}
await browser.close();
await site.close();
console.log(`${list.length} page(s) x ${MODES.length} media settings, ${failures} failure(s)`);
process.exit(failures === 0 ? 0 : 1);
