#!/bin/bash
# Runs every tests/ui/test_*.js scenario in its own JavaScriptCore process, against the real
# ../../ui (app.v7.js, themes.js, index.html, styles.v7.css). The tests live OUTSIDE ui/ because
# tauri.conf.json frontendDist=../ui embeds every file under ui/ into the app binary.
# Usage: bash tests/ui/run.sh [test_file_substring] [scenario_substring]
#        APP=/path/to/other/app.v7.js bash tests/ui/run.sh   (run the suite against another copy)
#        FIXTURE=worktree bash tests/ui/run.sh               (replay another recorded backend)
#        FIXTURE_DIR=/path/elsewhere bash tests/ui/run.sh     (fixtures kept in another folder)
#        UI_DIR=/path/to/ui bash tests/ui/run.sh              (another copy of the whole ui/ folder)
# Exit status: 0 only if every scenario printed PASS.
set -u
JSC=${JSC:-/System/Library/Frameworks/JavaScriptCore.framework/Versions/Current/Helpers/jsc}
DIR="$(cd "$(dirname "$0")" && pwd)"
UI_DIR="${UI_DIR:-$(cd "$DIR/../../ui" && pwd)}"
if [ ! -f "$UI_DIR/app.v7.js" ] || [ ! -f "$UI_DIR/index.html" ]; then
  echo "FAIL  app UI not found in $UI_DIR (set UI_DIR)"; exit 1
fi
FILTER_FILE="${1:-}"
FILTER_SC="${2:-}"
FIXTURE_DIR="${FIXTURE_DIR:-$DIR/fixtures}"
if [ ! -f "$FIXTURE_DIR/${FIXTURE:-head}.json" ]; then
  echo "FAIL  fixture not found: $FIXTURE_DIR/${FIXTURE:-head}.json (set FIXTURE_DIR)"; exit 1
fi
PRELUDE="$(mktemp -t espada-ui-prelude)"
trap 'rm -f "$PRELUDE"' EXIT
LIB_DIR="${ESPADA_MODULES_DIR:-$HOME/Downloads/for mac espada}"
printf 'var HARNESS_TESTS_DIR = %s;\nvar HARNESS_UI_DIR = %s;\nvar HARNESS_MODE = "test";\nvar HARNESS_APP = %s;\nvar HARNESS_FIXTURE = %s;\nvar HARNESS_FIXTURE_DIR = %s;\nvar HARNESS_LIB_DIR = %s;\n' "\"$DIR\"" "\"$UI_DIR\"" "\"${APP:-}\"" "\"${FIXTURE:-head}\"" "\"$FIXTURE_DIR\"" "\"$LIB_DIR\"" > "$PRELUDE"

# Syntax smoke: every UI script and test file must parse.
syntax_fail=0
for f in "$UI_DIR"/app.v7.js "$UI_DIR"/themes.js "$DIR"/harness.js "$DIR"/_run.js "$DIR"/test_*.js; do
  [ -e "$f" ] || continue
  if ! out=$("$JSC" -e "checkSyntax('$f')" 2>&1); then
    echo "FAIL  syntax: $f"; echo "$out" | sed 's/^/      /' | head -5; syntax_fail=1
  fi
done

pass=0; fail=0; failed=()
for t in "$DIR"/test_*.js; do
  [ -e "$t" ] || continue
  name="$(basename "$t")"
  if [ -n "$FILTER_FILE" ] && [[ "$name" != *"$FILTER_FILE"* ]]; then continue; fi
  scenarios=$("$JSC" "$PRELUDE" "$DIR/harness.js" "$t" "$DIR/_run.js" -- --list 2>&1)
  if ! grep -q '^SCENARIO' <<<"$scenarios"; then
    echo "FAIL  $name: could not list scenarios"; echo "$scenarios" | head -20
    fail=$((fail+1)); failed+=("$name"); continue
  fi
  while IFS=$'\t' read -r tag idx sc; do
    [ "$tag" = "SCENARIO" ] || continue
    if [ -n "$FILTER_SC" ] && [[ "$sc" != *"$FILTER_SC"* ]]; then continue; fi
    out=$("$JSC" "$PRELUDE" "$DIR/harness.js" "$t" "$DIR/_run.js" -- "$idx" 2>&1)
    if grep -q "^PASS	" <<<"$out"; then
      pass=$((pass+1)); echo "ok    $name :: $sc"
    else
      fail=$((fail+1)); failed+=("$name :: $sc")
      echo "FAIL  $name :: $sc"
      echo "$out" | sed 's/^/      /' | head -40
    fi
  done <<<"$scenarios"
done
echo "—— $pass passed, $fail failed"
[ "$fail" -eq 0 ] && [ "$syntax_fail" -eq 0 ]
