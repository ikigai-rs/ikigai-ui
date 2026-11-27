// The demo door for the workspace's VoiceOver checks (ledger #3953), as Playwright's
// globalSetup. The workspace's close and maximize need the demo's real `/workspace/{iri}` route
// and its edit Sink, which the static axe site cannot answer; scripts/screenshots.sh starts the
// same door for the Chromium specs. It is started here, not in .github/workflows/ci.yml, so the
// job's steps stay as they are: it builds `ikigai-ui-demo` (fixtures only), starts it on a free
// loopback port and puts its URL in DEMO_BASE, which workspace.spec.mjs reads. A door that does
// not build or start is not hidden: DEMO_FAILED says why, workspace.spec.mjs fails with it, and
// button.spec.mjs runs as before. DEMO_BASE already set uses that door instead.
import { spawn, spawnSync } from "node:child_process";
import { join, resolve } from "node:path";

export default async function demoDoor() {
  if (process.env.DEMO_BASE) return undefined;
  const root = resolve(import.meta.dirname, "..");
  const built = spawnSync("cargo", ["build", "-p", "ikigai-ui-demo"], { cwd: root, stdio: "inherit" });
  if (built.status !== 0) {
    process.env.DEMO_FAILED = `cargo build -p ikigai-ui-demo exited ${built.status ?? built.signal}`;
    return undefined;
  }
  const port = 20000 + Math.floor(Math.random() * 20000);
  const demo = spawn(join(root, "target/debug/ikigai-ui-demo"), [`127.0.0.1:${port}`], {
    cwd: root,
    stdio: "inherit",
  });
  const base = `http://127.0.0.1:${port}`;
  for (let i = 0; i < 150; i++) {
    try {
      if ((await fetch(base + "/")).ok) {
        process.env.DEMO_BASE = base;
        return async () => demo.kill();
      }
    } catch {
      // not listening yet
    }
    await new Promise((r) => setTimeout(r, 200));
  }
  demo.kill();
  process.env.DEMO_FAILED = `the demo door did not answer ${base}/ within 30 s`;
  return undefined;
}
