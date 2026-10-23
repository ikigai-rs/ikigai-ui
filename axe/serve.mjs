// A loopback file server for a static site that ikigai_ui::testkit::site wrote: media types
// from the site's _types.json (else by extension), and the door default CSP (UI conventions
// D6) on every answer, so an inline script or style attribute fails as it would on a real
// door. `/_status/{code}` answers that status with a short plain body, for the shell smoke
// test's refusals.
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
  const server = createServer(async (req, res) => {
    const path = decodeURIComponent(new URL(req.url, "http://localhost").pathname);
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
    close: () => new Promise((ok) => server.close(ok)),
  };
}
