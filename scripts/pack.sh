#!/bin/sh
# Lists or packs the first-party plugins in kumikit-plugin-git.
# Usage:
#   sh scripts/pack.sh --list
#   sh scripts/pack.sh <out-dir> [plugin-id ...]
set -eu

KUMIKIT_BIN=${KUMIKIT_BIN:-kumikit}
PLUGINS="git/plugin.toml ui-git/package/plugin.toml"

read_field() {
  python3 -c 'import sys, tomllib; print(tomllib.load(open(sys.argv[1], "rb"))[sys.argv[2]])' "$1" "$2"
}

if [ "${1:-}" = "--list" ]; then
  for m in $PLUGINS; do
    id=$(read_field "$m" id)
    ver=$(read_field "$m" version)
    short=${id#kumikit.}
    printf '%s %s %s-v%s\n' "$id" "$ver" "$short" "$ver"
  done
  exit 0
fi

OUT=${1:?usage: scripts/pack.sh <out-dir> [plugin-id ...]}
shift
WANT=" $* "

mkdir -p "$OUT"
STAGE=$(mktemp -d "${TMPDIR:-/tmp}/kk-pack.XXXXXX")
trap 'rm -rf "$STAGE"' EXIT

want_id() {
  [ "$WANT" = "  " ] && return 0
  case "$WANT" in
    *" $1 "*) return 0 ;;
    *) return 1 ;;
  esac
}

id=$(read_field "git/plugin.toml" id)
ver=$(read_field "git/plugin.toml" version)
if want_id "$id"; then
  wasm="target/wasm32-wasip2/release/kumikit_git.wasm"
  if [ ! -f "$wasm" ]; then
    echo "missing $wasm — run: cargo build --target wasm32-wasip2 --release -p kumikit-git" >&2
    exit 1
  fi
  d="$STAGE/git"
  mkdir -p "$d"
  cp git/plugin.toml git/README.md "$d/"
  cp -r git/tools "$d/"
  cp "$wasm" "$d/plugin.wasm"
  "$KUMIKIT_BIN" plugin pack "$d" -o "$OUT/$id-$ver.kkp"
fi

id=$(read_field "ui-git/package/plugin.toml" id)
ver=$(read_field "ui-git/package/plugin.toml" version)
if want_id "$id"; then
  if [ ! -f "ui-git/package/dist/panel.js" ]; then
    echo "missing ui-git/package/dist/panel.js — run: (cd ui-git && npm run build)" >&2
    exit 1
  fi
  "$KUMIKIT_BIN" plugin pack ui-git/package -o "$OUT/$id-$ver.kkp"
fi
