#!/bin/bash
# Records real-backend responses for tests/ui fixtures.
#
#   bash tests/ui/recorder/record.sh [--src head|worktree] [--fixture NAME] [test_file_filter]
#   APP=/path/app.v7.js bash tests/ui/recorder/record.sh ...   (record the calls another app copy makes)
#
# 1. Copies backend sources (git HEAD via `git show`, read-only; or the working tree) plus this
#    recorder into a temp build dir and builds it (cargo --offline, pinned by the app's Cargo.lock).
# 2. Runs the UI scenarios in "collect" mode; every IPC call without a recorded reply is written
#    out as a request.
# 3. The recorder executes those requests with the real module functions against the real
#    library and merges the replies into tests/ui/fixtures/NAME.json. Repeats until nothing is missing.
# Never writes inside src-tauri/ or the module library.
set -euo pipefail
JSC=${JSC:-/System/Library/Frameworks/JavaScriptCore.framework/Versions/Current/Helpers/jsc}
HERE="$(cd "$(dirname "$0")" && pwd)"
TESTS="$(cd "$HERE/.." && pwd)"
REPO="$(cd "$TESTS/../.." && pwd)"   # tests/ui → repo root
UI_DIR="${UI_DIR:-$REPO/ui}"
LIB_DIR="${ESPADA_MODULES_DIR:-$HOME/Downloads/for mac espada}"
SRC=head; FIX=""; FILTER=""
while [ $# -gt 0 ]; do
  case "$1" in
    --src) SRC="$2"; shift 2;;
    --fixture) FIX="$2"; shift 2;;
    *) FILTER="$1"; shift;;
  esac
done
[ -n "$FIX" ] || FIX="$SRC"
BUILD="${TMPDIR:-/tmp}/espada-ui-recorder-$SRC"
mkdir -p "$BUILD/src"
cp "$HERE/Cargo.toml" "$BUILD/"
cp "$HERE/src/main.rs" "$BUILD/src/"
if [ "$SRC" = "head" ]; then
  for f in modules text books crossrefs; do git -C "$REPO" show "HEAD:src-tauri/src/$f.rs" > "$BUILD/src/$f.rs"; done
  git -C "$REPO" show HEAD:src-tauri/Cargo.lock > "$BUILD/Cargo.lock"
  LABEL="HEAD $(git -C "$REPO" rev-parse --short HEAD)"
else
  for f in modules text books crossrefs; do
    cp "$REPO/src-tauri/src/$f.rs" "$BUILD/src/"
    if [ -d "$REPO/src-tauri/src/$f" ]; then rm -rf "$BUILD/src/$f"; cp -R "$REPO/src-tauri/src/$f" "$BUILD/src/$f"; fi
  done
  cp "$REPO/src-tauri/Cargo.lock" "$BUILD/Cargo.lock"
  LABEL="worktree $(date +%Y-%m-%d)"
fi
export PATH="$HOME/.cargo/bin:$PATH"
(cd "$BUILD" && (cargo build --offline -q 2>/dev/null || cargo build -q))
REC="$BUILD/target/debug/recorder"
FIXTURE_DIR="${FIXTURE_DIR:-$TESTS/fixtures}"
OUT="$FIXTURE_DIR/$FIX.json"
WORK="$(mktemp -d -t espada-ui-record)"
trap 'rm -rf "$WORK"' EXIT

for round in 1 2 3 4 5 6 7 8 9 10; do
  reqs=()
  n=0
  for t in "$TESTS"/test_*.js; do
    name="$(basename "$t")"
    if [ -n "$FILTER" ] && [[ "$name" != *"$FILTER"* ]]; then continue; fi
    printf 'var HARNESS_TESTS_DIR = "%s";\nvar HARNESS_UI_DIR = "%s";\nvar HARNESS_MODE = "test";\nvar HARNESS_LIB_DIR = "%s";\n' "$TESTS" "$UI_DIR" "$LIB_DIR" > "$WORK/prelude-list.js"
    while IFS=$'\t' read -r tag idx sc; do
      [ "$tag" = "SCENARIO" ] || continue
      n=$((n+1))
      req="$WORK/req-$round-$n.json"
      printf 'var HARNESS_TESTS_DIR = "%s";\nvar HARNESS_UI_DIR = "%s";\nvar HARNESS_MODE = "collect";\nvar HARNESS_REQUESTS_OUT = "%s";\nvar HARNESS_APP = "%s";\nvar HARNESS_FIXTURE = "%s";\nvar HARNESS_FIXTURE_DIR = "%s";\nvar HARNESS_LIB_DIR = "%s";\n' "$TESTS" "$UI_DIR" "$req" "${APP:-}" "$FIX" "$FIXTURE_DIR" "$LIB_DIR" > "$WORK/prelude.js"
      "$JSC" "$WORK/prelude.js" "$TESTS/harness.js" "$t" "$TESTS/_run.js" -- "$idx" >/dev/null 2>&1 || true
      if [ -s "$req" ] && [ "$(cat "$req")" != "[]" ]; then reqs+=("$req"); fi
    done < <("$JSC" "$WORK/prelude-list.js" "$TESTS/harness.js" "$t" "$TESTS/_run.js" -- --list 2>/dev/null)
  done
  if [ ${#reqs[@]} -eq 0 ]; then echo "record: fixtures complete after $((round-1)) round(s) -> $OUT"; exit 0; fi
  echo "record: round $round, ${#reqs[@]} scenario(s) with missing replies"
  "$REC" "$OUT" "$LABEL" "${reqs[@]}"
done
echo "record: still missing replies after 10 rounds" >&2
exit 1
