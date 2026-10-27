#!/usr/bin/env bash
# Build llvm-bolt from LLVM monorepo into ~/.local/bolt (macOS experimental).
# Homebrew's llvm bottle does NOT ship llvm-bolt on Darwin.
set -euo pipefail

PREFIX="${BOLT_PREFIX:-$HOME/.local/bolt}"
SRC="${BOLT_SRC:-$HOME/.cache/espada-llvm-project}"
BUILD="${BOLT_BUILD:-$HOME/.cache/espada-bolt-build}"
# Match installed Homebrew LLVM major when possible
TAG="${LLVM_TAG:-llvmorg-22.1.8}"

export PATH="/opt/homebrew/bin:/opt/homebrew/opt/llvm/bin:$PATH"

echo "==> BOLT install prefix: $PREFIX"
echo "==> LLVM source: $SRC ($TAG)"

if [[ ! -d "$SRC/.git" ]]; then
  mkdir -p "$(dirname "$SRC")"
  git clone --depth 1 --branch "$TAG" https://github.com/llvm/llvm-project.git "$SRC"
else
  echo "Source already present."
fi

mkdir -p "$BUILD"
cd "$BUILD"

cmake -G Ninja "$SRC/llvm" \
  -DCMAKE_BUILD_TYPE=Release \
  -DCMAKE_INSTALL_PREFIX="$PREFIX" \
  -DLLVM_ENABLE_PROJECTS="bolt" \
  -DLLVM_TARGETS_TO_BUILD="AArch64;X86" \
  -DLLVM_ENABLE_ASSERTIONS=OFF \
  -DLLVM_INCLUDE_TESTS=OFF \
  -DLLVM_INCLUDE_EXAMPLES=OFF \
  -DLLVM_INCLUDE_BENCHMARKS=OFF \
  -DLLVM_ENABLE_ZLIB=ON \
  -DLLVM_ENABLE_ZSTD=ON

echo "==> Building llvm-bolt (this can take a long time)…"
ninja -j"$(sysctl -n hw.ncpu 2>/dev/null || echo 4)" llvm-bolt merge-fdata 2>&1 || ninja -j"$(sysctl -n hw.ncpu)" bolt

mkdir -p "$PREFIX/bin"
# Install bolt binaries from build tree
find "$BUILD" -type f \( -name 'llvm-bolt' -o -name 'merge-fdata' -o -name 'perf2bolt' -o -name 'bolt-heatmap' \) 2>/dev/null | while read -r f; do
  if [[ -x "$f" ]]; then
    cp -f "$f" "$PREFIX/bin/" || true
    echo "Installed $(basename "$f")"
  fi
done

if [[ -x "$PREFIX/bin/llvm-bolt" ]]; then
  echo "OK: $PREFIX/bin/llvm-bolt"
  "$PREFIX/bin/llvm-bolt" --version 2>&1 | head -5 || true
else
  echo "WARN: llvm-bolt binary not found after build."
  echo "BOLT on macOS/Mach-O is experimental; instrumentation PGO is the reliable path."
  exit 1
fi
