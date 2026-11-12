#!/usr/bin/env bash
# The README's GPUI screenshots (ledger #3528): `scripts/gpui-screenshots.sh`, on a machine with
# a screen. The test platform draws no pixels (ui PR 38), so this starts the real native demo,
# `ikigai-ui-demo-native`, once per theme, and captures each of its windows (gallery, elements,
# contact, opened in that order) to docs/screenshots/gpui-{screen}-{theme}.png.
#
# macOS: `screencapture -l <window id>`, the ids from CGWindowList for the demo's process (the
# terminal needs Screen Recording permission). Linux: `xdotool` for the ids and ImageMagick's
# `import -window` (X11), when the native demo has a Linux window backend; it does not yet.
# CI does not run this: it needs screen access. Say in the PR who ran it and where.
set -euo pipefail
cd "$(dirname "$0")/.."
out=docs/screenshots
mkdir -p "$out"
screens=(gallery elements contact)

cargo build -p ikigai-ui-gpui --bin ikigai-ui-demo-native

windows() { # the demo's on-screen window ids, oldest first
  case "$(uname)" in
    Darwin)
      swift - "$1" <<'SWIFT'
import CoreGraphics
let pid = Int(CommandLine.arguments[1])!
let list = CGWindowListCopyWindowInfo([.optionOnScreenOnly], kCGNullWindowID) as! [[String: Any]]
let ids = list.filter { ($0[kCGWindowOwnerPID as String] as? Int) == pid && ($0[kCGWindowLayer as String] as? Int) == 0 }
  .compactMap { $0[kCGWindowNumber as String] as? Int }.sorted()
for id in ids { print(id) }
SWIFT
      ;;
    *) xdotool search --onlyvisible --pid "$1" ;;
  esac
}

capture() { # window id, file
  case "$(uname)" in
    Darwin) screencapture -x -o -l "$1" "$2" ;;
    *) import -window "$1" "$2" ;;
  esac
}

for theme in light dark; do
  port=$((20000 + RANDOM % 20000))
  target/debug/ikigai-ui-demo-native "127.0.0.1:$port" "$theme" &
  demo=$!
  trap 'kill $demo 2>/dev/null || true' EXIT
  ids=()
  for _ in $(seq 1 60); do
    ids=($(windows "$demo" || true))
    [[ ${#ids[@]} -ge ${#screens[@]} ]] && break
    sleep 0.5
  done
  if [[ ${#ids[@]} -lt ${#screens[@]} ]]; then
    echo "found ${#ids[@]} window(s) of ${#screens[@]} for $theme; is there a screen?" >&2
    exit 1
  fi
  sleep 3 # the worker's first load and the palette read
  for i in "${!screens[@]}"; do
    file="$out/gpui-${screens[$i]}-$theme.png"
    capture "${ids[$i]}" "$file"
    command -v oxipng >/dev/null && oxipng -q -o 4 --strip safe "$file"
    echo "wrote $file"
  done
  kill "$demo"; wait "$demo" 2>/dev/null || true
done
