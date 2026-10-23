// The shell script's behavior in a real browser (UI conventions D12, D13, D14):
// `node axe/shell-smoke.mjs <site-dir>`, over the demo site `cargo run --example axe_site`
// writes. Each check prints ok or FAIL; the exit status is 1 on any failure.
//
// Controls the demo pages do not carry are added with page.evaluate and handed to htmx with
// htmx.process: the test driver is not the page, so the door CSP does not apply to it.
import { chromium } from "playwright";
import { serve } from "./serve.mjs";

const dir = process.argv[2];
if (!dir) {
  console.error("usage: node axe/shell-smoke.mjs <site-dir>");
  process.exit(2);
}
const site = await serve(dir);
const browser = await chromium.launch();
let failures = 0;

async function check(name, run) {
  const context = await browser.newContext();
  const page = await context.newPage();
  const errors = [];
  page.on("pageerror", (e) => errors.push(e.message));
  try {
    const why = await run(page);
    if (errors.length) throw new Error(`page error: ${errors.join("; ")}`);
    if (why) throw new Error(why);
    console.log(`ok   ${name}`);
  } catch (e) {
    failures += 1;
    console.log(`FAIL ${name}: ${e.message}`);
  }
  await context.close();
}

async function add(page, html, parent = "#main") {
  await page.evaluate(
    ([html, parent]) => {
      const host = document.querySelector(parent);
      host.insertAdjacentHTML("beforeend", html);
      window.htmx.process(host);
    },
    [html, parent],
  );
}

const home = site.base + "/home-auto.html";

await check("a refusal is shown as text after the control that asked", async (page) => {
  await page.goto(home);
  await page.click("text=Load details");
  const note = page.locator("button + p.ui-note");
  await note.waitFor();
  const text = await note.textContent();
  if (text !== "not found") return `the note says ${JSON.stringify(text)}`;
  if (await page.locator("text=Load details").isDisabled()) return "a 404 disabled the control";
  await page.click("text=Load details");
  await page.waitForTimeout(200);
  if ((await page.locator("p.ui-note").count()) !== 1) return "a second refusal stacked a note";
});

await check("a 403 disables the control, and markup in a refusal is text", async (page) => {
  await page.goto(home);
  await add(page, '<button id="deny" hx-get="/_status/403" hx-target="#detail">Deny</button>');
  await page.click("#deny");
  await page.locator("#deny + p.ui-note").waitFor();
  if (!(await page.locator("#deny").isDisabled())) return "the button is still enabled";
  if ((await page.getAttribute("#deny", "aria-disabled")) !== "true") return "no aria-disabled";
  const note = await page.textContent("#deny + p.ui-note");
  if (!note.includes("urn:cap:example:403")) return `the note says ${JSON.stringify(note)}`;
});

await check("a control that swaps into itself gets the note in place", async (page) => {
  await page.goto(home);
  await add(page, '<div id="lazy" hx-get="/_status/500" hx-trigger="load">loading…</div>');
  await page.locator("#lazy > p.ui-note").waitFor();
  const text = await page.textContent("#lazy");
  if (text.includes("loading")) return "the placeholder is still there";
});

await check("a failure outside #main is an alert in #status; a quiet one says nothing", async (page) => {
  await page.goto(home);
  await add(page, '<span data-ui-quiet><button id="badge" hx-get="/_status/503">Badge</button></span>', "header");
  await page.click("#badge");
  await page.waitForTimeout(300);
  if ((await page.textContent("#status")) !== "") return "a quiet element wrote to #status";
  await add(page, '<button id="loud" hx-get="/_status/500">Loud</button>', "header");
  await page.click("#loud");
  await page.locator('#status p[role="alert"]').waitFor();
});

await check("a swap into #main moves focus to its heading", async (page) => {
  await page.goto(home);
  await add(page, '<a id="nav" href="/about-auto.html" hx-get="/about-auto.html" hx-select="main > *" hx-target="#main">About</a>', "header");
  await page.focus("#nav");
  await page.keyboard.press("Enter");
  await page.waitForFunction(() => document.activeElement && document.activeElement.tagName === "H1");
  const text = await page.evaluate(() => document.activeElement.textContent);
  if (text !== "About") return `focus is on ${JSON.stringify(text)}`;
});

// htmx 2 itself restores focus to an element with the same id after a swap (measured: this
// check passes with the script's afterSettle handler removed, and the #main one does not). It
// is kept as the contract: the script's part is the case htmx cannot cover, never <body>.
await check("focus survives a swap that replaced the focused element", async (page) => {
  await page.goto(home);
  // The served page's own #theme select, made to replace itself with the server's copy.
  await page.evaluate(() => {
    const select = document.getElementById("theme");
    select.setAttribute("hx-get", "/home-auto.html");
    select.setAttribute("hx-select", "#theme");
    select.setAttribute("hx-swap", "outerHTML");
    select.setAttribute("hx-trigger", "change");
    window.htmx.process(select);
    window.__old = select;
  });
  await page.focus("#theme");
  await page.selectOption("#theme", "dark");
  await page.waitForFunction(() => document.getElementById("theme") !== window.__old);
  await page.waitForTimeout(200);
  const where = await page.evaluate(() => {
    const a = document.activeElement;
    return a ? `${a.tagName}#${a.id}` : "nothing";
  });
  if (where !== "SELECT#theme") return `focus is on ${where}`;
});

await check("theme= is applied and remembered, and the chooser changes it", async (page) => {
  await page.goto(home + "?theme=dark");
  if ((await page.getAttribute("html", "data-theme")) !== "dark") return "theme=dark not applied";
  await page.goto(home);
  if ((await page.getAttribute("html", "data-theme")) !== "dark") return "the choice was not remembered";
  await page.selectOption("#theme", "high-contrast");
  if ((await page.getAttribute("html", "data-theme")) !== "high-contrast") return "the chooser did not apply";
  await page.selectOption("#theme", "auto");
  await page.goto(home);
  if ((await page.getAttribute("html", "data-theme")) !== "auto") return "auto did not forget the choice";
});

// ---- live views (D9): the push stream stand-in in serve.mjs -----------------------------------
const live = site.base + "/live-auto.html";

async function subscribed(paths) {
  const want = JSON.stringify(paths);
  for (let i = 0; i < 50; i += 1) {
    if (site.push.streams().some((s) => JSON.stringify(s) === want)) return true;
    await new Promise((ok) => setTimeout(ok, 100));
  }
  return false;
}
const countText = (page) => page.textContent("#count-focus");

await check("a live view subscribes to what it shows, and only its own cut refetches it", async (page) => {
  await page.goto(live);
  if (!(await subscribed(["/_live/count"]))) return `no stream for /_live/count: ${JSON.stringify(site.push.streams())}`;
  await page.focus("#elsewhere");
  const before = await countText(page);
  site.push.cut(["/_live/other"]);
  await page.waitForTimeout(300);
  if ((await countText(page)) !== before) return "a cut on another path refetched it";
  site.push.cut(["/_live/count"]);
  await page.waitForFunction((b) => document.getElementById("count-focus").textContent !== b, before);
  if ((await page.textContent("#status")) !== "") return "a refetch wrote to #status";
  const where = await page.evaluate(() => document.activeElement && document.activeElement.id);
  if (where !== "elsewhere") return `focus moved to ${where}`;
});

await check("focus inside a live view is kept across its refetch", async (page) => {
  await page.goto(live);
  if (!(await subscribed(["/_live/count"]))) return "no stream";
  await page.focus("#count-focus");
  const before = await countText(page);
  site.push.cut(["/_live/count"]);
  await page.waitForFunction((b) => document.getElementById("count-focus").textContent !== b, before);
  await page.waitForTimeout(100);
  const where = await page.evaluate(() => document.activeElement && document.activeElement.id);
  if (where !== "count-focus") return `focus is on ${where}`;
});

await check("a resync refetches every live view", async (page) => {
  await page.goto(live);
  if (!(await subscribed(["/_live/count"]))) return "no stream";
  const before = await countText(page);
  site.push.resync();
  await page.waitForFunction((b) => document.getElementById("count-focus").textContent !== b, before);
});

await check("a live view's failed refetch is quiet and stacks nothing", async (page) => {
  await page.goto(live);
  if (!(await subscribed(["/_live/count"]))) return "no stream";
  site.live.fail = true;
  try {
    site.push.cut(["/_live/count"]);
    await page.waitForTimeout(300);
    site.push.cut(["/_live/count"]);
    await page.waitForTimeout(300);
  } finally {
    site.live.fail = false;
  }
  if ((await page.textContent("#status")) !== "") return "a failed refetch wrote to #status";
  if ((await page.locator("p.ui-note").count()) !== 0) return "a failed refetch left a note";
  if (!(await page.locator("#count-focus").isEnabled())) return "the view was disabled";
});

await check("a page with no live view opens no stream", async (page) => {
  const opened = site.push.streams().length;
  await page.goto(home);
  await page.waitForTimeout(300);
  // home declares no live view, so nothing subscribes even though its head names the stream.
  if (site.push.streams().length > opened) return "a page with no live view opened a stream";
});

await browser.close();
await site.close();
console.log(`${failures} failure(s)`);
process.exit(failures === 0 ? 0 : 1);
