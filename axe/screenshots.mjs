// The README's HTML screenshots (ledger #3528), generated, never hand-taken:
// `node axe/screenshots.mjs <demo-base-url> <out-dir> [--check <committed-dir>]`.
// `scripts/screenshots.sh` starts the demo door and runs this; nobody needs to call it directly.
//
// It opens the demo (fixtures only: the gallery's examples, the demo's own contact contract, no
// real data) in headless Chromium, in light and in dark (the media emulation axe/check.mjs uses;
// the pages are `auto`, so the token set follows it), and writes each shot as lossless WebP,
// at quality 0.9, a fraction of the PNG's size.
//
// Staleness without pixels. Fonts, antialiasing and GPUs differ between machines, so the images
// are not compared. Instead each shot's INPUTS are hashed: the page's DOM after the interaction
// (canvas elements and the renderer's drawn marker taken out) and the text of every stylesheet it
// links. The hashes go to manifest.json beside the images. `--check` compares the fresh manifest
// with the committed one and fails naming every shot whose inputs changed (or whose WebGPU
// availability changed, which changes the scene's image and its alt text); the fresh images are
// in <out-dir> for CI to upload as an artifact, to be committed.
import { chromium } from "playwright";
import { createHash } from "node:crypto";
import { mkdir, readFile, writeFile } from "node:fs/promises";
import { join } from "node:path";

const [base, out, flag, committed] = process.argv.slice(2);
if (!base || !out || (flag && (flag !== "--check" || !committed))) {
  console.error("usage: node axe/screenshots.mjs <demo-base-url> <out-dir> [--check <committed-dir>]");
  process.exit(2);
}

const MODES = ["light", "dark"];
const WIDTH = 1000;
const MAX_HEIGHT = 1400;

// Each shot: a page, what to do on it, and what to capture (a selector, or the page).
const SHOTS = [
  {
    name: "lens-board-page-2",
    path: "/lens/name/demo-board/resource/urn%3Aiki%3Aui%3Aexample%3Agraph%3Alenses",
    act: async (page) => {
      await page.getByLabel("Column page (1–1000)").fill("2");
      await page.getByRole("checkbox", { name: "Status: Todo", exact: true }).check();
      await page.getByRole("button", { name: "Show", exact: true }).click();
      await page.waitForURL("**page=2**");
      await page.getByText("Page 2 of 2; 4 cards in this answer.", { exact: true }).waitFor();
      const todo = page.locator('.ui-columns > section[aria-label="Todo"]');
      if (await todo.getByRole("link", { name: "Thread renderer", exact: true }).count()
          || !(await todo.getByRole("link", { name: "Timeline renderer", exact: true }).count())) {
        throw new Error("the board did not page within its Todo column");
      }
    },
  },
  // The whole gallery, the Media card (the photo, a visual, the scene) included.
  { name: "gallery", path: "/page/gallery", maxHeight: 3200 },
  { name: "elements", path: "/page/elements" },
  {
    name: "contact-refusal",
    path: "/page/contact",
    // A submission the contact Sink refuses: the address is not one. The shell lands the
    // refusal beside the email field and says it in #status.
    act: async (page) => {
      const form = page.locator("form[data-ui-form]").first();
      await form.locator("[name=name]").fill("Ada Example");
      await form.locator("[name=email]").fill("ada-at-example");
      const topic = form.locator("[name=topic]").first();
      if ((await topic.evaluate((e) => e.tagName)) === "SELECT") await topic.selectOption("question");
      else await form.locator("[name=topic][value=question]").check();
      await form.locator("[name=message]").fill("Is the lighthouse open on Sundays?");
      await form.locator("button[type=submit], input[type=submit]").first().click();
      await page.locator("form[data-ui-form] [aria-invalid=true]").first().waitFor({ timeout: 10000 });
      await page.waitForFunction(() =>
        [...document.querySelectorAll("p.ui-note")].some((n) => n.textContent.trim() !== ""),
      );
      await page.locator("[name=email]").first().blur();
    },
    clip: "form[data-ui-form]",
  },
  { name: "support", path: "/page/support" },
  {
    name: "scene",
    path: "/page/gallery",
    act: async (page) => {
      // The renderer marks a figure it drew; with WebGPU off (below) it never does, and the
      // replacement is the scene. Wait in case a browser draws anyway: the manifest records it.
      await page
        .waitForSelector("figure[data-ui-scene][data-ui-scene-drawn]", { timeout: 5000 })
        .catch(() => {});
    },
    clip: "figure[data-ui-scene]",
  },
];

async function inputs(page) {
  return page.evaluate(async () => {
    const root = document.documentElement.cloneNode(true);
    for (const c of root.querySelectorAll("canvas")) c.remove();
    for (const f of root.querySelectorAll("[data-ui-scene-drawn]")) f.removeAttribute("data-ui-scene-drawn");
    const css = [];
    for (const l of document.querySelectorAll("link[rel=stylesheet]")) {
      css.push(await (await fetch(l.href)).text());
    }
    return { dom: root.outerHTML, css };
  });
}

async function webp(browser, png) {
  const page = await browser.newPage();
  const data = await page.evaluate(async (b64) => {
    const img = new Image();
    img.src = "data:image/png;base64," + b64;
    await img.decode();
    const c = document.createElement("canvas");
    c.width = img.naturalWidth;
    c.height = img.naturalHeight;
    c.getContext("2d").drawImage(img, 0, 0);
    // Lossy at 0.9: text stays crisp and a page is a few tens of kilobytes.
    return c.toDataURL("image/webp", 0.9).split(",")[1];
  }, png.toString("base64"));
  await page.close();
  return Buffer.from(data, "base64");
}

await mkdir(out, { recursive: true });
// No WebGPU. Headless Chromium's screenshots do not carry a WebGPU canvas's pixels (on a Mac
// with an adapter the renderer draws, and the shot shows an empty box), and Linux CI has no
// adapter at all. So every machine shoots the scene's replacement, the accessible form the
// html face shows whenever the canvas cannot be drawn, and the alt text says so. Nothing
// stands in for the canvas.
const browser = await chromium.launch({ args: ["--disable-features=WebGPU,Vulkan", "--disable-gpu"] });
const manifest = {};
let errors = 0;
for (const shot of SHOTS) {
  for (const mode of MODES) {
    const file = `${shot.name}-${mode}.webp`;
    const context = await browser.newContext({ viewport: { width: WIDTH, height: 900 }, deviceScaleFactor: 2 });
    const page = await context.newPage();
    page.on("pageerror", (e) => {
      errors += 1;
      console.log(`FAIL ${file}: page error: ${e.message}`);
    });
    await page.emulateMedia({ colorScheme: mode, reducedMotion: "reduce" });
    await page.goto(base + shot.path, { waitUntil: "networkidle" });
    if (shot.act) await shot.act(page);
    let png;
    if (shot.clip) {
      png = await page.locator(shot.clip).first().screenshot({ animations: "disabled" });
    } else {
      const height = Math.min(shot.maxHeight ?? MAX_HEIGHT, await page.evaluate(() => document.documentElement.scrollHeight));
      png = await page.screenshot({ clip: { x: 0, y: 0, width: WIDTH, height }, fullPage: true, animations: "disabled" });
    }
    const drawn = (await page.locator("figure[data-ui-scene][data-ui-scene-drawn]").count()) > 0;
    const { dom, css } = await inputs(page);
    const hash = createHash("sha256").update(dom).update("\0").update(css.join("\0")).digest("hex");
    const image = await webp(browser, png);
    await writeFile(join(out, file), image);
    manifest[file] = { inputs: hash, ...(shot.name === "scene" ? { webgpu: drawn } : {}) };
    console.log(`wrote ${file}: ${image.length} bytes${shot.name === "scene" ? (drawn ? ", WebGPU canvas" : ", no WebGPU adapter: the replacement") : ""}`);
    await context.close();
  }
}
await browser.close();
await writeFile(join(out, "manifest.json"), JSON.stringify(manifest, null, 2) + "\n");

if (flag === "--check") {
  let old = {};
  try {
    old = JSON.parse(await readFile(join(committed, "manifest.json"), "utf8"));
  } catch {
    old = {};
  }
  const stale = [];
  for (const [file, now] of Object.entries(manifest)) {
    const then = old[file];
    if (!then) stale.push(`${file}: missing`);
    else if (then.inputs !== now.inputs) stale.push(`${file}: the page or its stylesheets changed`);
    else if (then.webgpu !== now.webgpu) stale.push(`${file}: WebGPU was ${then.webgpu} and is ${now.webgpu}`);
  }
  for (const file of Object.keys(old)) if (!manifest[file]) stale.push(`${file}: no longer generated`);
  for (const s of stale) console.log(`STALE ${s}`);
  if (stale.length) {
    console.log(`${stale.length} stale screenshot(s): regenerate with scripts/screenshots.sh, or commit the job's artifact`);
    process.exit(1);
  }
  console.log("screenshots current");
}
process.exit(errors === 0 ? 0 : 1);
