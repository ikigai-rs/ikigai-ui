#!/usr/bin/env bash
# Regenerate the README's HTML screenshots (ledger #3528): `scripts/screenshots.sh`.
# `scripts/screenshots.sh --check` writes them to target/screenshots instead and fails, naming
# each one, if docs/screenshots/ is stale (axe/screenshots.mjs says how staleness is judged).
#
# It builds and starts the demo door (fixtures only) on a free loopback port, installs the
# pinned Playwright from axe/package-lock.json if it is not there, and shoots light and dark.
set -euo pipefail
cd "$(dirname "$0")/.."

out=docs/screenshots
check=()
if [[ "${1:-}" == "--check" ]]; then
  out=target/screenshots
  check=(--check docs/screenshots)
fi

[[ -d axe/node_modules/playwright ]] || (cd axe && npm ci --no-audit --no-fund)
(cd axe && npx playwright install chromium >/dev/null)

cargo build -p ikigai-ui-demo
port=$((20000 + RANDOM % 20000))
target/debug/ikigai-ui-demo "127.0.0.1:$port" &
demo=$!
trap 'kill $demo 2>/dev/null || true' EXIT
for _ in $(seq 1 100); do
  curl -fsS -o /dev/null "http://127.0.0.1:$port/" && break
  sleep 0.2
done

node axe/screenshots.mjs "http://127.0.0.1:$port" "$out" ${check[@]+"${check[@]}"}
