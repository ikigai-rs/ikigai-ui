#!/usr/bin/env bash
# Regenerate native fixture images on a machine with a screen (never in CI).
# macOS: swift, screencapture, Screen Recording access for the invoking terminal.
# Linux/FreeBSD: an X11 desktop (or Xwayland), xdotool and ImageMagick's import.
# Both: cargo and oxipng. Only the demo's windows are captured, never the desktop.
set -euo pipefail
cd "$(dirname "$0")/.."
out=docs/screenshots
screens=(gallery elements contact)
platform=$(uname)
features=()

need() {
  command -v "$1" >/dev/null || { echo "missing required tool: $1" >&2; exit 1; }
}
need cargo
need oxipng # Always compress and strip metadata, not just when a tool happens to be installed.
case "$platform" in
  Darwin)
    need swift
    need screencapture
    if ! swift -e 'import CoreGraphics; import Darwin; exit(CGPreflightScreenCaptureAccess() ? 0 : 1)'; then
      echo "Screen Recording access required: allow this terminal in macOS System Settings, then restart it." >&2
      exit 1
    fi
    ;;
  Linux|FreeBSD)
    need xdotool
    need import
    [[ -n "${DISPLAY:-}" ]] || { echo "X11 DISPLAY required (Xwayland works); run from a desktop terminal." >&2; exit 1; }
    features=(--features x11)
    # import captures X11 windows. Use that same backend even in a Wayland desktop session.
    unset WAYLAND_DISPLAY
    ;;
  *) echo "window capture is supported on macOS and X11 Linux/FreeBSD" >&2; exit 1 ;;
esac

cargo build -p ikigai-ui-gpui --bin ikigai-ui-demo-native ${features[@]+"${features[@]}"}
mkdir -p "$out"
staging=$(mktemp -d "$out/.gpui-capture.XXXXXX")
demo=
cleanup() {
  if [[ -n "$demo" ]]; then
    kill "$demo" 2>/dev/null || true
    wait "$demo" 2>/dev/null || true
  fi
  rm -rf "$staging"
}
trap cleanup EXIT

windows() { # the demo's on-screen window ids, oldest first
  case "$platform" in
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
    *) xdotool search --onlyvisible --pid "$1" | sort -n ;;
  esac
}

capture() { # window id, file
  case "$platform" in
    Darwin) screencapture -x -o -l "$1" "$2" ;;
    *) import -window "$1" "$2" ;;
  esac
}

for theme in light dark; do
  port=$((20000 + RANDOM % 20000))
  target/debug/ikigai-ui-demo-native "127.0.0.1:$port" "$theme" &
  demo=$!
  ids=()
  for _ in $(seq 1 60); do
    ids=($(windows "$demo" || true))
    [[ ${#ids[@]} -ge ${#screens[@]} ]] && break
    kill -0 "$demo" 2>/dev/null || { echo "native demo exited before opening its windows" >&2; exit 1; }
    sleep 0.5
  done
  if [[ ${#ids[@]} -ne ${#screens[@]} ]]; then
    echo "found ${#ids[@]} window(s) of ${#screens[@]} for $theme; is there an unlocked screen?" >&2
    exit 1
  fi
  sleep 3 # the worker's first load and the palette read
  for i in "${!screens[@]}"; do
    file="$staging/gpui-${screens[$i]}-$theme.png"
    capture "${ids[$i]}" "$file"
    # screencapture can exit successfully after saying it could not create an image.
    [[ -s "$file" ]] || { echo "no image captured for ${screens[$i]} ($theme); check screen access" >&2; exit 1; }
    oxipng -q -o 4 --strip safe "$file"
  done
  kill "$demo"; wait "$demo" 2>/dev/null || true
  demo=
done
# Failed captures leave the previously committed set alone.
for file in "$staging"/*.png; do
  mv "$file" "$out/"
  echo "wrote $out/$(basename "$file")"
done
