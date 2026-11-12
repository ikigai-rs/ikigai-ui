// The shell script's behavior in a real browser (UI conventions D12, D13, D14):
// `node axe/shell-smoke.mjs <site-dir>`, over the demo site `cargo run --example axe_site`
// writes. Each check prints ok or FAIL; the exit status is 1 on any failure.
//
// Controls the demo pages do not carry are added with page.evaluate and handed to htmx with
// htmx.process: the test driver is not the page, so the door CSP does not apply to it.
import { chromium } from "playwright";
import { serve, CSP } from "./serve.mjs";

const dir = process.argv[2];
if (!dir) {
  console.error("usage: node axe/shell-smoke.mjs <site-dir>");
  process.exit(2);
}
const site = await serve(dir);
const browser = await chromium.launch();
let failures = 0;

async function check(name, run, on = browser) {
  const context = await on.newContext();
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

// One status region (D13): a note in #main is plain text with no role, the control it belongs
// to points at it with aria-describedby, and #status says the same sentence, naming the control.
const linked = (page, control) =>
  page.evaluate((control) => {
    const c = document.querySelector(control);
    const ids = (c.getAttribute("aria-describedby") || "").split(/\s+/).filter(Boolean);
    const notes = ids.map((id) => document.getElementById(id)).filter((n) => n && n.matches("p.ui-note"));
    return { ids, notes: notes.map((n) => n.id) };
  }, control);
async function oneRegion(page, note, control, said) {
  const role = await page.locator(note).getAttribute("role");
  if (role !== null) return `the note has role=${role}`;
  if ((await page.locator('#main [role="status"], #main [role="alert"], #main [aria-live]').count()) !== 0)
    return "a live region inside #main";
  const id = await page.locator(note).getAttribute("id");
  if (!id) return "the note has no id";
  const link = await linked(page, control);
  if (JSON.stringify(link.ids) !== JSON.stringify([id])) return `${control} aria-describedby is ${JSON.stringify(link.ids)}, not [${id}]`;
  const mirror = await page.textContent("#status");
  if (mirror !== said) return `#status says ${JSON.stringify(mirror)}, not ${JSON.stringify(said)}`;
  if ((await page.locator('#status [role="alert"]').count()) !== 0) return "the #status mirror is an alert";
  return null;
}

// The button itself: its name is also in the #status mirror, so text= would find two.
const details = 'button[hx-get="/demo/fragment/missing"]';
await check("a refusal is shown as text after the control that asked", async (page) => {
  await page.goto(home);
  await page.click(details);
  const note = page.locator("button + p.ui-note");
  await note.waitFor();
  const text = await note.textContent();
  if (text !== "not found") return `the note says ${JSON.stringify(text)}`;
  if (await page.locator(details).isDisabled()) return "a 404 disabled the control";
  const first = await oneRegion(page, "button + p.ui-note", details, "Load details: not found");
  if (first) return first;
  const oldId = await note.getAttribute("id");
  await page.click(details);
  await page.waitForTimeout(200);
  if ((await page.locator("p.ui-note").count()) !== 1) return "a second refusal stacked a note";
  // The replaced note's link went with it; the control points at the new one only.
  const again = await oneRegion(page, "button + p.ui-note", details, "Load details: not found");
  if (again) return `after a second refusal: ${again}`;
  const newId = await note.getAttribute("id");
  if (newId !== oldId && (await page.locator(`[aria-describedby~="${oldId}"]`).count()) !== 0)
    return "a link to the replaced note is left";
});

// A 403 marks its control aria-disabled and nothing more (ledger #3547): `disabled` would take
// a button out of the tab order, so a keyboard or screen-reader user could no longer reach it or
// hear why it is unavailable. It stays focusable, announces as unavailable, its description is
// the refusal, and the shell blocks its activation: a click, Enter, Space or a form submit it
// would make sends nothing.
await check("a 403 leaves its control focusable, aria-disabled, and inert; markup in a refusal is text", async (page) => {
  await page.goto(home);
  await add(
    page,
    '<form id="deny-form" action="/_status/200" method="get"><button id="before" type="button">Before</button>' +
      '<button id="deny" hx-get="/_status/403" hx-target="#detail">Deny <b>it</b></button></form>',
  );
  await page.click("#deny");
  await page.locator("#deny + p.ui-note").waitFor();
  if (await page.locator("#deny").isDisabled()) return "the button is disabled, so out of the tab order";
  if ((await page.getAttribute("#deny", "aria-disabled")) !== "true") return "no aria-disabled";
  const note = await page.textContent("#deny + p.ui-note");
  if (!note.includes("urn:cap:example:403")) return `the note says ${JSON.stringify(note)}`;
  const region = await oneRegion(page, "#deny + p.ui-note", "#deny", "Deny it: refused: needs urn:cap:example:403");
  if (region) return region;
  // Its accessible description is the refusal.
  const described = await page.evaluate(() =>
    (document.getElementById("deny").getAttribute("aria-describedby") || "")
      .split(/\s+/)
      .filter(Boolean)
      .map((id) => document.getElementById(id).textContent)
      .join(" "),
  );
  if (described !== note) return `its description is ${JSON.stringify(described)}, not the refusal`;
  // Tab reaches it.
  await page.focus("#before");
  await page.keyboard.press("Tab");
  if ((await page.evaluate(() => document.activeElement && document.activeElement.id)) !== "deny")
    return "Tab does not reach the refused button";
  // Activating it does nothing and sends nothing.
  const sent = [];
  page.on("request", (r) => sent.push(r.url()));
  const at = page.url();
  await page.keyboard.press("Enter");
  await page.keyboard.press("Space");
  await page.click("#deny");
  await page.evaluate(() => document.getElementById("deny-form").requestSubmit(document.getElementById("deny")));
  await page.waitForTimeout(300);
  if (sent.length) return `activating the refused button sent ${JSON.stringify(sent)}`;
  if (page.url() !== at) return `activating the refused button navigated to ${page.url()}`;
  if ((await page.locator("p.ui-note").count()) !== 1) return "activating the refused button changed its note";
  if ((await page.evaluate(() => document.activeElement && document.activeElement.id)) !== "deny")
    return "activating the refused button moved focus";
  return null;
});

// htmx's eval and script evaluation are off in a SERVED page (ledger #3525): the shell's
// htmx-config meta is what htmx read, not only what the HTML says. A swapped-in <script src> is
// same-origin, so the CSP alone would let htmx run it: only allowScriptTags:false stops it. An
// hx-vals js: expression fires htmx's evalDisallowedError only under allowEval:false (the CSP
// would refuse the eval too, but silently, as a different error).
await check("htmx eval and script tags are off in a served page", async (page) => {
  await page.route("**/_probe/**", (route) => {
    const path = new URL(route.request().url()).pathname;
    if (path === "/_probe/ran.js")
      return route.fulfill({ status: 200, contentType: "text/javascript", headers: { "content-security-policy": CSP }, body: "window.__ran = true;" });
    if (path === "/_probe/fragment")
      return route.fulfill({ status: 200, contentType: "text/html; charset=utf-8", headers: { "content-security-policy": CSP }, body: '<p id="landed">landed</p><script src="/_probe/ran.js"></script>' });
    return route.fulfill({ status: 200, contentType: "text/html; charset=utf-8", headers: { "content-security-policy": CSP }, body: '<p id="sent">sent</p>' });
  });
  await page.goto(home);
  const config = await page.evaluate(() => ({ eval: window.htmx.config.allowEval, scripts: window.htmx.config.allowScriptTags }));
  if (config.eval !== false || config.scripts !== false) return `htmx.config in force: ${JSON.stringify(config)}`;
  await add(page, '<div id="probe-swap" hx-get="/_probe/fragment" hx-trigger="load"></div>');
  await page.locator("#landed").waitFor();
  await page.waitForTimeout(300);
  if (await page.evaluate(() => window.__ran === true)) return "a swapped-in <script> ran";
  await page.evaluate(() => {
    window.__disallowed = 0;
    document.addEventListener("htmx:evalDisallowedError", () => { window.__disallowed += 1; });
  });
  await add(page, '<button id="probe-eval" hx-get="/_probe/vals" hx-vals="js:{v: (window.__evaluated = 1)}" hx-target="this">Probe</button>');
  await page.click("#probe-eval");
  await page.waitForTimeout(300);
  const after = await page.evaluate(() => ({ evaluated: window.__evaluated === 1, disallowed: window.__disallowed }));
  if (after.evaluated) return "an hx-vals js: expression was evaluated";
  if (after.disallowed < 1) return "htmx did not refuse the js: expression as allowEval:false does";
});

await check("a control that swaps into itself gets the note in place, and loses the link when it goes", async (page) => {
  await page.goto(home);
  await add(page, '<section id="lazy" aria-label="Details" hx-get="/_status/500" hx-trigger="load">loading…</section>');
  await page.locator("#lazy > p.ui-note").waitFor();
  const text = await page.textContent("#lazy");
  if (text.includes("loading")) return "the placeholder is still there";
  const linkedNow = await oneRegion(page, "#lazy > p.ui-note", "#lazy", "Details: refused: needs urn:cap:example:500");
  if (linkedNow) return linkedNow;
  // A later answer that swaps the note away takes the link with it.
  await add(page, '<button id="fill" hx-get="/about-auto.html" hx-select="main h1" hx-target="#lazy">Fill</button>');
  await page.click("#fill");
  await page.locator("#lazy h1").waitFor();
  await page.waitForTimeout(100);
  if ((await page.locator("#lazy p.ui-note").count()) !== 0) return "the note is still there";
  const left = await page.getAttribute("#lazy", "aria-describedby");
  if (left !== null) return `#lazy still has aria-describedby=${JSON.stringify(left)}`;
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
// is kept as the contract: the script's part is the case htmx cannot cover (the next check).
await check("focus survives a swap that replaced the focused element", async (page) => {
  await page.goto(home);
  // The shell's own theme select, made to replace itself with the server's copy.
  await page.evaluate(() => {
    const select = document.getElementById("ui-theme");
    select.setAttribute("hx-get", "/home-auto.html");
    select.setAttribute("hx-select", "#ui-theme");
    select.setAttribute("hx-swap", "outerHTML");
    select.setAttribute("hx-trigger", "change");
    window.htmx.process(select);
    window.__old = select;
  });
  await page.focus("#ui-theme");
  await page.selectOption("#ui-theme", "dark");
  await page.waitForFunction(() => document.getElementById("ui-theme") !== window.__old);
  await page.waitForTimeout(200);
  const where = await page.evaluate(() => {
    const a = document.activeElement;
    return a ? `${a.tagName}#${a.id}` : "nothing";
  });
  if (where !== "SELECT#ui-theme") return `focus is on ${where}`;
  // The server's copy says auto; the chooser that replaced it shows the theme in force.
  const shown = await page.inputValue("#ui-theme");
  if (shown !== "dark") return `the swapped-in chooser shows ${shown}`;
});

// The case htmx cannot cover: the focused control had no id, so nothing in the new content is
// "the same element". The script gives focus to the region that took it.
await check("a swap that removed a focused control with no id gives focus to its region", async (page) => {
  await page.goto(home);
  await add(page, '<section id="holder" aria-label="Holder"><button hx-get="/about-auto.html" hx-select="main h1" hx-target="#holder">Replace me</button></section>');
  await page.focus("#holder button");
  await page.keyboard.press("Enter");
  await page.locator("#holder h1").waitFor();
  await page.waitForTimeout(200);
  const where = await page.evaluate(() => {
    const a = document.activeElement;
    return a ? `${a.tagName}#${a.id}` : "nothing";
  });
  if (where !== "SECTION#holder") return `focus is on ${where}`;
});

// A page nobody has focused (ledger #2899): a poll or a lazily loaded region swaps in the
// background, and that is not focus lost. Focus stays on <body> and the page does not scroll
// to whatever swapped. The page is made tall and scrolled first, and the swapping element is
// handed to htmx only then, so the first swap cannot come before the scroll. The swapping
// element keeps a fixed size: it sits above the viewport, and content that grows there moves
// scrollY by scroll anchoring (measured, 40-60px), which keeps the view still and is not a jump.
const idle = (page, where, html, swapping) =>
  page.evaluate(
    ([where, html, swapping]) => {
      const spacer = document.createElement("div");
      spacer.style.height = "4000px";
      document.getElementById("main").appendChild(spacer);
      window.__settled = 0;
      document.addEventListener("htmx:afterSettle", () => (window.__settled += 1));
      const host = document.querySelector(where);
      host.insertAdjacentHTML("afterbegin", html);
      const el = document.querySelector(swapping);
      el.style.display = "block";
      el.style.height = "3em";
      el.style.overflow = "hidden";
      window.scrollTo(0, 2000);
      window.htmx.process(host);
      return window.scrollY;
    },
    [where, html, swapping],
  );
async function untouched(page, scrolled, swaps) {
  if (scrolled < 1000) return `the page did not scroll for the test (scrollY ${scrolled})`;
  await page.waitForFunction((n) => window.__settled >= n, swaps);
  await page.waitForTimeout(200);
  const after = await page.evaluate(() => {
    const a = document.activeElement;
    return { on: a ? (a === document.body ? "BODY" : `${a.tagName}#${a.id}`) : "nothing", y: window.scrollY };
  });
  if (after.on !== "BODY") return `focus moved to ${after.on}`;
  if (after.y !== scrolled) return `the page scrolled from ${scrolled} to ${after.y}`;
  return null;
}

await check("an idle page's poll leaves focus on <body> and the scroll where it was", async (page) => {
  await page.goto(home);
  const scrolled = await idle(
    page,
    "header",
    '<span data-ui-quiet><span id="badge-poll" hx-get="/about-auto.html" hx-select="main h1" hx-trigger="load, every 150ms">0</span></span>',
    "#badge-poll",
  );
  return untouched(page, scrolled, 3);
});

await check("an idle page's lazily loaded region leaves focus on <body> and the scroll where it was", async (page) => {
  await page.goto(home);
  const scrolled = await idle(
    page,
    "#main",
    '<section id="lazy-region" aria-label="Lazy" hx-get="/about-auto.html" hx-select="main h1" hx-trigger="load">loading…</section>',
    "#lazy-region",
  );
  return untouched(page, scrolled, 1);
});

await check("theme= is applied and remembered, and the chooser changes it", async (page) => {
  await page.goto(home + "?theme=dark");
  if ((await page.getAttribute("html", "data-theme")) !== "dark") return "theme=dark not applied";
  await page.goto(home);
  if ((await page.getAttribute("html", "data-theme")) !== "dark") return "the choice was not remembered";
  // The server rendered auto selected; the chooser shows the remembered theme instead.
  if ((await page.inputValue("#ui-theme")) !== "dark") return "the chooser does not show the remembered theme";
  await page.selectOption("#ui-theme", "high-contrast");
  if ((await page.getAttribute("html", "data-theme")) !== "high-contrast") return "the chooser did not apply";
  await page.selectOption("#ui-theme", "auto");
  await page.goto(home);
  if ((await page.getAttribute("html", "data-theme")) !== "auto") return "auto did not forget the choice";
});

// The shell's chooser (ledger #2835): every header has it, a choice applies at once, is kept in
// this browser's localStorage (auto keeps nothing), and holds across a reload.
const KEY = "ikigai-ui:theme";
const stored = (page) => page.evaluate((key) => window.localStorage.getItem(key), KEY);
await check("the header's chooser applies each theme at once and keeps it across a reload", async (page) => {
  await page.goto(home);
  if ((await page.locator("header select#ui-theme[data-ui-theme]").count()) !== 1) return "no chooser in the header";
  if (await page.locator("header .ui-theme-apply").isVisible()) return "the Apply button shows with the script running";
  for (const theme of ["light", "dark", "high-contrast", "auto"]) {
    await page.selectOption("#ui-theme", theme);
    if ((await page.getAttribute("html", "data-theme")) !== theme) return `${theme}: data-theme not applied`;
    const kept = await stored(page);
    if (kept !== (theme === "auto" ? null : theme)) return `${theme}: localStorage holds ${JSON.stringify(kept)}`;
    if (!page.url().endsWith("/home-auto.html")) return `${theme}: the page navigated to ${page.url()}`;
    await page.reload();
    if ((await page.getAttribute("html", "data-theme")) !== theme) return `${theme}: not kept across a reload`;
    if ((await page.inputValue("#ui-theme")) !== theme) return `${theme}: the chooser shows ${await page.inputValue("#ui-theme")} after a reload`;
    if ((await stored(page)) !== kept) return `${theme}: the reload changed the remembered value`;
  }
});

await check("the chooser is reached and changed from the keyboard", async (page) => {
  await page.goto(home);
  let reached = false;
  for (let i = 0; i < 10 && !reached; i += 1) {
    await page.keyboard.press("Tab");
    reached = await page.evaluate(() => document.activeElement && document.activeElement.id === "ui-theme");
  }
  if (!reached) return "Tab never reached the chooser";
  const name = await page.evaluate(() => document.activeElement.labels[0].textContent);
  if (name !== "Theme") return `the chooser is labeled ${JSON.stringify(name)}`;
  await page.keyboard.press("ArrowDown");
  if ((await page.getAttribute("html", "data-theme")) !== "light") return "ArrowDown did not choose light";
});

// With no script, the form is the chooser: Apply shows, and submitting it asks for the page
// with theme= (serve.mjs answers as a door does, with the page in that theme).
const noScript = { newContext: () => browser.newContext({ javaScriptEnabled: false }) };
await check("with no script, the chooser submits theme= and the page comes back in it", async (page) => {
  await page.goto(home);
  if (!(await page.locator("header .ui-theme-apply").isVisible())) return "no Apply button with no script";
  await page.selectOption("#ui-theme", "dark");
  await Promise.all([page.waitForURL(/\?theme=dark$/), page.click("header .ui-theme-apply")]);
  if ((await page.getAttribute("html", "data-theme")) !== "dark") return "the page did not come back dark";
  if ((await page.inputValue("#ui-theme")) !== "dark") return "the chooser does not show dark";
}, noScript);

// ---- live views (D9): the push stream stand-in in serve.mjs -----------------------------------
// The embedded 3D scene (ledger #3514) on the gallery. With WebGPU and an adapter the canvas
// draws (shown, marked drawn, and not blank); with none, or with no script, the canvas stays
// hidden and the replacement (every edge as text, linked from the caption) is what shows.
// CI's headless Chromium may have no adapter: the check says which path it took.
const gallery = site.base + "/gallery.html";
const replacementShows = async (page) => {
  const table = page.locator(".ui-scene-replacement table");
  if (!(await table.isVisible())) return "the replacement table is not visible";
  if ((await page.locator(".ui-scene-replacement tr").count()) !== 8) return "the replacement lacks a row per edge";
  const href = await page.getAttribute(".ui-scene figcaption a.ui-scene-text", "href");
  if (!href || !(await page.locator(href.replace(/\./g, "\\.")).isVisible())) return `the caption's link ${href} does not reach the replacement`;
};
const canvasHidden = async (page) => {
  if (await page.locator("canvas.ui-scene-canvas").isVisible()) return "the canvas shows with nothing drawn on it";
  if (await page.getAttribute("figure.ui-scene", "data-ui-scene-drawn") !== null) return "marked drawn";
};
// The renderer's one fetch stays on the page's origin whatever the page's attribute says
// (ledger #3525): `/\\host` and `/<tab>/host` start with one `/`, but the URL parser reads both
// as `//host`. A stand-in WebGPU (any adapter will do: the fetch comes before any drawing) makes
// the renderer reach its fetch in a browser with none; the door CSP would block the request, so
// the check watches for the attempt, as a request or as a CSP violation, not for a response.
for (const steer of ["/\\evil.example/datum", "/\t/evil.example/datum"]) {
  await check(`the scene renderer never fetches off-origin (data-ui-scene=${JSON.stringify(steer)})`, async (page) => {
    await page.addInitScript(() => {
      window.__offOrigin = [];
      document.addEventListener("securitypolicyviolation", (e) => window.__offOrigin.push(e.blockedURI));
      const gpu = {
        requestAdapter: async () => ({ requestDevice: async () => ({}) }),
        getPreferredCanvasFormat: () => "bgra8unorm",
      };
      Object.defineProperty(Navigator.prototype, "gpu", { configurable: true, get: () => gpu });
    });
    const requests = [];
    page.on("request", (r) => { if (new URL(r.url()).host === "evil.example") requests.push(r.url()); });
    await page.route("**/*", (route) => {
      if (new URL(route.request().url()).host === "evil.example") return route.abort();
      return route.continue();
    });
    await page.route(gallery, async (route) => {
      const response = await route.fetch();
      const body = (await response.text()).replace(/data-ui-scene="[^"]*"/, `data-ui-scene="${steer.replace("\t", "&#9;")}"`);
      await route.fulfill({ response, body });
    });
    await page.goto(gallery);
    await page.waitForTimeout(500);
    const blocked = await page.evaluate(() => window.__offOrigin.filter((u) => u.includes("evil.example")));
    if (requests.length || blocked.length) return `it reached for ${[...requests, ...blocked].join(", ")}`;
  });
}
await check("a scene draws its canvas where WebGPU is, else its replacement is the scene", async (page) => {
  await page.goto(gallery);
  const gpu = await page.evaluate(async () => !!(navigator.gpu && (await navigator.gpu.requestAdapter())));
  if (!gpu) {
    console.log("     (no WebGPU adapter in this browser: checked the absent path)");
    await page.waitForTimeout(500);
    return (await canvasHidden(page)) || (await replacementShows(page));
  }
  const drawn = await page
    .waitForSelector("figure.ui-scene[data-ui-scene-drawn]", { timeout: 5000 })
    .then(() => true, () => false);
  if (!drawn) return "WebGPU is present but the scene was not drawn";
  if (!(await page.locator("canvas.ui-scene-canvas").isVisible())) return "drawn, but the canvas is hidden";
  // Not blank: copy the presented frame to a 2D canvas and look for a painted pixel.
  const painted = await page.evaluate(() => new Promise((ok) => requestAnimationFrame(() => {
    const src = document.querySelector("canvas.ui-scene-canvas");
    const c = document.createElement("canvas");
    c.width = src.width; c.height = src.height;
    const g = c.getContext("2d");
    g.drawImage(src, 0, 0);
    const d = g.getImageData(0, 0, c.width, c.height).data;
    for (let i = 3; i < d.length; i += 4) if (d[i] !== 0) return ok(true);
    ok(false);
  })));
  if (!painted) return "the canvas is blank";
  console.log("     (WebGPU adapter present: checked the drawn path)");
  // The keyboard selects, and #status says it.
  await page.focus("canvas.ui-scene-canvas");
  await page.keyboard.press("n");
  const said = await page.textContent("#status");
  if (!/^Selected /.test(said || "")) return `#status says ${JSON.stringify(said)} after N`;
  return replacementShows(page);
});
await check("with WebGPU absent, a scene's replacement is what shows", async (page) => {
  await page.addInitScript(() => Object.defineProperty(Navigator.prototype, "gpu", { get: () => undefined }));
  await page.goto(gallery);
  await page.waitForTimeout(500);
  return (await canvasHidden(page)) || (await replacementShows(page));
});
await check("with no script, a scene's replacement is what shows", async (page) => {
  await page.goto(gallery);
  return (await canvasHidden(page)) || (await replacementShows(page));
}, { newContext: () => browser.newContext({ javaScriptEnabled: false }) });

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

// ---- the three timing and visibility gaps (ledger #2151), each driven at an exact moment ------
const until = async (test, what) => {
  for (let i = 0; i < 50; i += 1) {
    if (test()) return null;
    await new Promise((ok) => setTimeout(ok, 100));
  }
  return what;
};

// The same sequence for a view whose indicator is itself (htmx's default, which the class check
// already covered) and for one whose indicator is elsewhere: the first is the harness's control.
for (const [name, indicator] of [["self", ""], ["elsewhere", 'hx-indicator="#spin"']]) {
  await check(`a cut during a refetch is sent again (indicator ${name})`, async (page) => {
    await page.goto(live);
    if (!(await subscribed(["/_live/count"]))) return "no stream";
    const id = `ind-${name}`;
    site.live.views[id] = { push: "/_live/count", attrs: indicator };
    site.live.reads[id] = 0;
    await add(
      page,
      `<span id="spin">working</span><div id="${id}" hx-get="/_live/view/${id}" hx-trigger="ikigai:cut" ` +
        `hx-swap="outerHTML" ${indicator} data-ui-push="/_live/count">${id} 0</div>`,
    );
    site.live.views[id].hold = true;
    site.push.cut(["/_live/count"]);
    if (await until(() => site.live.waiting[id] === 1, "x")) return "the first refetch never left";
    // The second cut lands while the first refetch is still on the wire.
    const counted = site.live.count;
    site.push.cut(["/_live/count"]);
    await page.waitForTimeout(300);
    // The neighbor on the same path takes that cut once; a held cut never loops through it.
    const neighbor = site.live.count - counted;
    site.live.release(id);
    if (neighbor !== 1) return `the neighbor view refetched ${neighbor} times for one cut`;
    if (await until(() => site.live.reads[id] >= 2, "x")) return `the held cut was dropped: ${site.live.reads[id]} read(s)`;
  });
}

await check("a stream reopened for a new path set misses no cut in the gap", async (page) => {
  await page.goto(live);
  if (!(await subscribed(["/_live/count"]))) return "no stream";
  const before = await countText(page);
  site.live.views.more = { push: "/_live/more" };
  site.push.hold();
  try {
    // A view that loads itself: its swap changes the declared paths, so the stream reopens.
    await add(page, '<div id="more" hx-get="/_live/view/more" hx-trigger="load" hx-swap="outerHTML" data-ui-push="/_live/more"></div>');
    const reopened = await until(
      () => site.push.held() === 1 && !site.push.streams().some((s) => s.includes("/_live/count")),
      `the stream did not reopen: open ${JSON.stringify(site.push.streams())}, held ${site.push.held()}`,
    );
    if (reopened) return reopened;
    // Nobody is listening now: the old stream is closed and the new one is not registered yet.
    site.push.cut(["/_live/count"]);
  } finally {
    site.push.release();
  }
  if (!(await subscribed(["/_live/count", "/_live/more"]))) return `not resubscribed: ${JSON.stringify(site.push.streams())}`;
  const changed = await page
    .waitForFunction((b) => document.getElementById("count-focus").textContent !== b, before, { timeout: 3000 })
    .then(() => true, () => false);
  if (!changed) return "the cut in the gap was missed";
});

await check("a refused stream is said once in #status, and only then", async (page) => {
  site.push.refuse = "3 subscriptions; a stream carries at most 2";
  try {
    await page.goto(live);
    await page.locator("#status p.ui-note").waitFor({ timeout: 5000 });
    await page.waitForTimeout(500);
    const notes = await page.locator("#status p.ui-note").count();
    if (notes !== 1) return `${notes} notes in #status`;
    if ((await page.locator("#main p.ui-note").count()) !== 0) return "a note in #main";
    // A new path set opens a new stream, refused too: the note stands, never said again.
    await page.evaluate(() => (window.__note = document.querySelector("#status p.ui-note")));
    site.live.views.refused = { push: "/_live/refused" };
    const opened = site.live.reads.refused || 0;
    await add(page, '<div id="refused" hx-get="/_live/view/refused" hx-trigger="load" hx-swap="outerHTML" data-ui-push="/_live/refused"></div>');
    if (await until(() => (site.live.reads.refused || 0) > opened, "x")) return "the new view never loaded";
    await page.waitForTimeout(500);
    const same = await page.evaluate(() => document.querySelector("#status p.ui-note") === window.__note);
    if (!same) return "a second refusal was said again";
  } finally {
    site.push.refuse = null;
  }
  // An accepted stream says nothing.
  await page.goto(live);
  if (!(await subscribed(["/_live/count"]))) return "no stream";
  await page.waitForTimeout(300);
  if ((await page.textContent("#status")) !== "") return "an accepted stream wrote to #status";
});

// The back-forward cache (ledger #2161). Playwright launches Chromium with bfcache off
// ("--disable-back-forward-cache"), and its default headless shell refuses it anyway
// ("BackForwardCacheDisabledForDelegate", measured), so this check runs in the full Chromium
// build (channel "chromium", which `playwright install chromium` installs too) without that one
// switch. It fails unless the page really came back from the cache (pageshow persisted): a page
// that was loaded again instead would refetch everything and prove nothing.
const bfcache = await chromium.launch({ channel: "chromium", ignoreDefaultArgs: ["--disable-back-forward-cache"] });
await check("a page back from the back-forward cache misses no cut made while it was away", async (page) => {
  await page.goto(live);
  if (!(await subscribed(["/_live/count"]))) return "no stream";
  const before = await countText(page);
  await page.evaluate(() => {
    window.__restored = false;
    window.addEventListener("pageshow", (e) => (window.__restored = e.persisted));
  });
  await page.goto(home);
  const closed = await until(
    () => !site.push.streams().some((s) => s.includes("/_live/count")),
    `the stream stayed open after pagehide: ${JSON.stringify(site.push.streams())}`,
  );
  if (closed) return closed;
  // Nobody is listening: the page sits in the cache with its stream closed.
  site.push.cut(["/_live/count"]);
  // A restore from the cache fires no load event: wait for the commit, then for pageshow.
  await page.goBack({ waitUntil: "commit" });
  const restored = await page
    .waitForFunction(() => window.__restored === true, null, { timeout: 5000 })
    .then(() => true, () => false);
  if (!restored) return "the page was not restored from the back-forward cache";
  if (!(await subscribed(["/_live/count"]))) return `not resubscribed: ${JSON.stringify(site.push.streams())}`;
  const changed = await page
    .waitForFunction((b) => document.getElementById("count-focus").textContent !== b, before, { timeout: 3000 })
    .then(() => true, () => false);
  if (!changed) return "the cut made while the page was in the back-forward cache was missed";
}, bfcache);
await bfcache.close();

await browser.close();
await site.close();
console.log(`${failures} failure(s)`);
process.exit(failures === 0 ? 0 : 1);
