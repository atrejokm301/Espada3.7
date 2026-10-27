#!/usr/bin/env bash
# Espada3.7 — optimized release (+ optional PGO / BOLT)
#
# Matches iOS EspadaSwift build-release.sh “goodies”:
#   fat LTO · -O3 · dead strip · single codegen unit · native CPU
#   CFLAGS -O3 -march=native for SQLite / C deps
#
# Tools expected (install once):
#   brew install llvm lld cmake ninja   # optional for PGO/BOLT
#   cargo install tauri-cli --version "^2"
#
# Usage:
#   ./build-release.sh              # fat LTO + O3 + strip + .app bundle
#   ./build-release.sh --pgo        # instrumentation PGO then fat LTO use
#   ./build-release.sh --pgo --bolt # PGO then try BOLT post-link
#   ./build-release.sh --open       # build then open the .app
#
set -euo pipefail
ROOT="$(cd "$(dirname "$0")" && pwd)"
cd "$ROOT"

source "${HOME}/.cargo/env" 2>/dev/null || true
export PATH="${HOME}/.local/bolt/bin:/opt/homebrew/opt/llvm/bin:/opt/homebrew/bin:${HOME}/.cargo/bin:${PATH}"
export DEVELOPER_DIR="${DEVELOPER_DIR:-/Applications/Xcode.app/Contents/Developer}"
export PATH="$DEVELOPER_DIR/usr/bin:$PATH"

PGO=0
BOLT=0
OPEN_APP=0
for arg in "$@"; do
  case "$arg" in
    --pgo) PGO=1 ;;
    --bolt) BOLT=1 ;;
    --open) OPEN_APP=1 ;;
    -h|--help)
      echo "Usage: $0 [--pgo] [--bolt] [--open]"
      exit 0
      ;;
  esac
done

# Kill any running Espada / dev UI server so we don't test the wrong binary
echo "==> Stopping old Espada / port 1420..."
pkill -x espada3_7 2>/dev/null || true
pkill -9 -x espada3_7 2>/dev/null || true
for pid in $(lsof -nP -iTCP:1420 -sTCP:LISTEN -t 2>/dev/null || true); do
  kill "$pid" 2>/dev/null || true
  kill -9 "$pid" 2>/dev/null || true
done
# Also quit .app if open
osascript -e 'tell application "Espada3.7" to quit' 2>/dev/null || true

export CFLAGS="-O3 -march=native -mtune=native -fstrict-aliasing -ffast-math -fno-exceptions -fno-rtti -fomit-frame-pointer"
export CXXFLAGS="$CFLAGS"
export MACOSX_DEPLOYMENT_TARGET="${MACOSX_DEPLOYMENT_TARGET:-11.0}"

# Fat LTO is in Cargo.toml [profile.release] lto="fat"
# dead_strip = macOS linker analog of DEAD_CODE_STRIPPING=YES
BASE_RUSTFLAGS=(
  "-C" "target-cpu=native"
  "-C" "llvm-args=--enable-machine-outliner"
  "-C" "link-arg=-dead_strip"
)

PGO_DIR="${ROOT}/target/pgo-raw"
PGO_MERGED="${ROOT}/target/pgo-data/merged.profdata"
MODULES_DIR="${ESPADA_MODULES:-$HOME/Downloads/for mac espada}"
APP_BIN_NAME="espada3_7"
LOG="${ROOT}/build-release.log"

echo "==> Toolchain"
echo "  rustc: $(rustc --version 2>/dev/null || echo missing)"
echo "  cargo tauri: $(cargo tauri --version 2>/dev/null || echo 'missing — install: cargo install tauri-cli --version ^2')"
echo "  llvm-profdata: $(command -v llvm-profdata || echo missing)"
echo "  modules: $MODULES_DIR"
echo "  profile: release · opt-level=3 · lto=fat · strip · dead_strip · cg=1"
echo "  UI embed: $ROOT/ui  (frontendDist — release does NOT use :1420)"
echo "  Log: $LOG"

# Sync UI mirrors so embedded assets stay consistent
cp -f "$ROOT/ui/styles.v7.css" "$ROOT/ui/styles.css"
cp -f "$ROOT/ui/app.v7.js" "$ROOT/ui/app.js"

# Touch index so tauri-build re-embeds frontend even if only CSS/JS mtimes matter
touch "$ROOT/ui/index.html"

run_pgo_train() {
  local harness="$1"
  rm -rf "$PGO_DIR"
  mkdir -p "$PGO_DIR" "$(dirname "$PGO_MERGED")"
  export LLVM_PROFILE_FILE="${PGO_DIR}/espada-%p-%m.profraw"
  echo "==> PGO train: harness ($harness)"
  if [[ -x "$harness" ]]; then
    "$harness" "$MODULES_DIR" 4
  else
    echo "Harness missing: $harness"
    return 1
  fi
  local app_bin="$ROOT/src-tauri/target/release/${APP_BIN_NAME}"
  if [[ -x "$app_bin" ]]; then
    echo "==> PGO train: short app launch"
    "$app_bin" >/dev/null 2>&1 &
    local pid=$!
    sleep 8
    kill "$pid" 2>/dev/null || true
    wait "$pid" 2>/dev/null || true
  fi
  echo "==> Merging profraw → $PGO_MERGED"
  local n
  n=$(find "$PGO_DIR" -name '*.profraw' 2>/dev/null | wc -l | tr -d ' ')
  echo "  found $n profraw files"
  if [[ "$n" -eq 0 ]]; then
    echo "ERROR: no profile data generated"
    return 1
  fi
  llvm-profdata merge -o "$PGO_MERGED" "$PGO_DIR"/*.profraw
  ls -lh "$PGO_MERGED"
}

{
if [[ "$PGO" -eq 1 ]]; then
  if ! command -v llvm-profdata >/dev/null; then
    echo "ERROR: llvm-profdata not found. Run: brew install llvm"
    exit 1
  fi

  echo "==> PGO pass 1: instrumented build"
  export RUSTFLAGS="${BASE_RUSTFLAGS[*]} -C profile-generate=${PGO_DIR}"
  mkdir -p "$PGO_DIR"
  export LLVM_PROFILE_FILE="${PGO_DIR}/espada-%p-%m.profraw"

  cd "$ROOT/src-tauri"
  cargo clean -p espada3_7 2>/dev/null || true
  cargo build --release --features pgo-harness --bin pgo_harness
  cargo build --release --bin "$APP_BIN_NAME"

  run_pgo_train "$ROOT/src-tauri/target/release/pgo_harness"

  echo "==> PGO pass 2: profile-use + fat LTO tauri bundle"
  export RUSTFLAGS="${BASE_RUSTFLAGS[*]} -C profile-use=${PGO_MERGED} -C llvm-args=-pgo-warn-missing-function"
  cargo clean -p espada3_7 2>/dev/null || true
  cargo tauri build
else
  export RUSTFLAGS="${RUSTFLAGS:-${BASE_RUSTFLAGS[*]}}"
  echo "RUSTFLAGS=$RUSTFLAGS"
  cd "$ROOT/src-tauri"
  # Force rebuild of app so frontendDist UI is re-embedded
  cargo clean -p espada3_7 2>/dev/null || true
  echo "==> cargo tauri build (release · fat LTO · O3)"
  cargo tauri build
fi
} 2>&1 | tee "$LOG"

APP="${ROOT}/src-tauri/target/release/bundle/macos/Espada3.7.app"
BIN_IN_APP="${APP}/Contents/MacOS/${APP_BIN_NAME}"
RELEASE_BIN="${ROOT}/src-tauri/target/release/${APP_BIN_NAME}"

TARGET_BIN="$BIN_IN_APP"
[[ -x "$TARGET_BIN" ]] || TARGET_BIN="$RELEASE_BIN"

if [[ ! -x "$TARGET_BIN" ]]; then
  echo "ERROR: no release binary at $BIN_IN_APP or $RELEASE_BIN"
  exit 1
fi

# Final strip of app executable only (like iOS strip of Espada binary, not .o)
if command -v xcrun >/dev/null && [[ -x "$TARGET_BIN" ]]; then
  echo "==> Final strip (app binary only) — dead symbols"
  BEFORE=$(stat -f%z "$TARGET_BIN" 2>/dev/null || wc -c <"$TARGET_BIN")
  xcrun strip -x "$TARGET_BIN" 2>/dev/null || xcrun strip "$TARGET_BIN" 2>/dev/null || true
  AFTER=$(stat -f%z "$TARGET_BIN" 2>/dev/null || wc -c <"$TARGET_BIN")
  echo "  size before strip: $BEFORE  after: $AFTER"
  # Ad-hoc re-sign after strip if .app exists
  if [[ -d "$APP" ]]; then
    codesign --force --deep --sign - "$APP" 2>/dev/null || true
  fi
fi

if [[ "$BOLT" -eq 1 ]]; then
  if ! command -v llvm-bolt >/dev/null; then
    echo "==> llvm-bolt not in PATH — trying install-bolt.sh"
    if [[ -x "$ROOT/scripts/install-bolt.sh" ]]; then
      "$ROOT/scripts/install-bolt.sh" || true
      export PATH="${HOME}/.local/bolt/bin:$PATH"
    fi
  fi
  if command -v llvm-bolt >/dev/null && [[ -x "$TARGET_BIN" ]]; then
    echo "==> BOLT: attempting post-link optimization on $TARGET_BIN"
    OUT="${TARGET_BIN}.bolt"
    set +e
    llvm-bolt "$TARGET_BIN" -o "$OUT" \
      --align-functions=32 \
      --icf=all \
      --reorder-blocks=ext-tsp \
      --reorder-functions=hfsort \
      --split-functions \
      --split-all-cold \
      --update-debug-sections \
      --dyno-stats 2>"${ROOT}/target/bolt.log"
    BOLT_RC=$?
    set -e
    if [[ $BOLT_RC -eq 0 && -x "$OUT" ]]; then
      cp -f "$TARGET_BIN" "${TARGET_BIN}.prebolt"
      mv -f "$OUT" "$TARGET_BIN"
      chmod +x "$TARGET_BIN"
      echo "BOLT applied. Original saved as ${TARGET_BIN}.prebolt"
      if [[ -d "$APP" ]]; then
        codesign --force --deep --sign - "$APP" 2>/dev/null || true
      fi
    else
      echo "BOLT failed or unsupported (see target/bolt.log)."
      tail -30 "${ROOT}/target/bolt.log" 2>/dev/null || true
    fi
  else
    echo "BOLT skipped (no llvm-bolt or binary)."
  fi
fi

echo ""
echo "========== Artifacts =========="
ls -lah "${ROOT}/src-tauri/target/release/bundle/macos/" 2>/dev/null || true
ls -lah "${ROOT}/src-tauri/target/release/bundle/dmg/" 2>/dev/null || true
if [[ -x "$BIN_IN_APP" ]]; then
  ls -lah "$BIN_IN_APP"
  file "$BIN_IN_APP"
  # Confirm UI embed markers (release uses tauri:// not :1420)
  if strings "$BIN_IN_APP" 2>/dev/null | grep -q "release-frost6\|set_frosted_window\|Asignaci"; then
    echo "  UI/command markers present in binary ✓"
  else
    echo "  (string markers may be compressed — OK if .app launches)"
  fi
fi
echo ""
echo "Done."
echo "  PGO:     $([[ $PGO -eq 1 ]] && echo yes || echo no)"
echo "  BOLT:    $([[ $BOLT -eq 1 ]] && echo attempted || echo no)"
echo "  LTO:     fat"
echo "  opt:     3 / O3 CFLAGS"
echo "  strip:   yes + -dead_strip"
echo "  CPU:     native (this Mac only)"
echo "  App:     $APP"
echo ""

if [[ "$OPEN_APP" -eq 1 && -d "$APP" ]]; then
  echo "==> Opening $APP"
  open "$APP"
fi
