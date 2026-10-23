// A loopback file server for a static site that ikigai_ui::testkit::site wrote: media types
// from the site's _types.json (else by extension), and the door default CSP (UI conventions
// D6) on every answer, so an inline script or style attribute fails as it would on a real
// door. `/_status/{code}` answers that status with a short plain body, for the shell smoke
// test's refusals.
//
// It also stands in for a door's push stream (D9) at `/_push`: an event stream that opens with
// `ready`, as the cli's does, and sends `cut` and `resync` when the test says so
// (`site.push.cut(paths)`, `site.push.resync()`; `site.push.streams()` lists each open stream's
// `path=` subscriptions). `/_live/count` is the live demo view's answer: the view, with a count
// that rises on each read, or a 500 while `site.live.fail` is set.
import { createServer } from "node:http";
import { readFile, readdir } from "node:fs/promises";
import { join, extname, resolve, sep } from "node:path";

export const CSP =
  "default-src 'self'; script-src 'self'; style-src 'self'; img-src 'self' data:; " +
  "connect-src 'self'; frame-ancestors 'none'; base-uri 'none'; form-action 'self'; " +
  "object-src 'none'";

const BY_EXTENSION = {
  ".html": "text/html; charset=utf-8",
  ".css": "text/css",
  ".js": "text/javascript",
  ".json": "application/json",
  ".svg": "image/svg+xml",
};

/// Every .html file under `dir`, as a site path.
export async function pages(dir, prefix = "") {
  const out = [];
  for (const entry of await readdir(dir, { withFileTypes: true })) {
    const rel = prefix + "/" + entry.name;
    if (entry.isDirectory()) out.push(...(await pages(join(dir, entry.name), rel)));
    else if (entry.name.endsWith(".html")) out.push(rel);
  }
  return out.sort();
}

/// Serve `dir` on 127.0.0.1, port 0. Resolves to `{ base, close }`.
export async function serve(dir) {
  const root = resolve(dir);
  let types = {};
  try {
    types = JSON.parse(await readFile(join(root, "_types.json"), "utf8"));
  } catch {
    types = {};
  }
  const streams = new Set();
  const live = { count: 0, fail: false };
  const send = (frame) => {
    for (const s of streams) s.res.write(frame);
  };
  const push = {
    streams: () => [...streams].map((s) => s.paths),
    cut: (paths) =>
      send(`event: cut\ndata: ${JSON.stringify({ thread: "urn:test:thread", sequence: 1, invalidated: [], more: 0, paths })}\n\n`),
    resync: () => send(`event: resync\ndata: {"reason":"dropped","dropped":1}\n\n`),
  };
  const server = createServer(async (req, res) => {
    const url = new URL(req.url, "http://localhost");
    const path = decodeURIComponent(url.pathname);
    if (path === "/_push") {
      res.writeHead(200, {
        "content-type": "text/event-stream",
        "cache-control": "no-store",
        "content-security-policy": CSP,
      });
      res.write("retry: 2000\n\nevent: ready\nid: 0\ndata: {}\n\n");
      const stream = { res, paths: url.searchParams.getAll("path") };
      streams.add(stream);
      req.on("close", () => streams.delete(stream));
      return;
    }
    if (path === "/_live/count") {
      if (live.fail) {
        res.writeHead(500, { "content-type": "text/plain", "content-security-policy": CSP }).end("the count is down");
        return;
      }
      live.count += 1;
      res
        .writeHead(200, { "content-type": "text/html; charset=utf-8", "content-security-policy": CSP })
        .end(
          `<div id="count" class="demo-live" hx-get="/_live/count" hx-trigger="ikigai:cut" hx-swap="outerHTML" data-ui-push="">` +
            `<button id="count-focus" type="button">Count ${live.count}</button></div>`,
        );
      return;
    }
    const status = /^\/_status\/(\d{3})$/.exec(path);
    if (status) {
      res
        .writeHead(Number(status[1]), { "content-type": "text/plain", "content-security-policy": CSP })
        .end(`refused: needs urn:cap:example:${status[1]}`);
      return;
    }
    const file = resolve(join(root, path));
    if (file !== root && !file.startsWith(root + sep)) {
      res.writeHead(403).end();
      return;
    }
    try {
      const body = await readFile(file);
      const type = types[path] || BY_EXTENSION[extname(path)] || "application/octet-stream";
      res.writeHead(200, { "content-type": type, "content-security-policy": CSP }).end(body);
    } catch {
      res.writeHead(404, { "content-type": "text/plain" }).end("not found");
    }
  });
  await new Promise((ok) => server.listen(0, "127.0.0.1", ok));
  return {
    base: `http://127.0.0.1:${server.address().port}`,
    push,
    live,
    close: () =>
      new Promise((ok) => {
        for (const s of streams) s.res.end();
        streams.clear();
        server.closeAllConnections();
        server.close(ok);
      }),
  };
}
