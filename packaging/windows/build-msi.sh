#!/usr/bin/env bash
# Cross-build flowplot.exe (x86_64-pc-windows-gnu) and pack Flowplot-Setup.msi via wixl.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
WIN="$ROOT/packaging/windows"
STAGE="$WIN/stage"
OUT="${1:-$WIN/Flowplot-Setup.msi}"

PREFIX="${FLOWPLOT_MINGW_PREFIX:-$HOME/.local/pkg}"
export PATH="$PREFIX/usr/bin:${PATH:-}"
export LD_LIBRARY_PATH="$PREFIX/usr/lib/x86_64-linux-gnu:${LD_LIBRARY_PATH:-}"

need() {
  command -v "$1" >/dev/null 2>&1 || {
    echo "missing tool: $1" >&2
    exit 1
  }
}

need cargo
need rustc
need x86_64-w64-mingw32-gcc
need wixl

rustup target add x86_64-pc-windows-gnu >/dev/null

echo "==> cargo build --release --target x86_64-pc-windows-gnu"
export CC_x86_64_pc_windows_gnu=x86_64-w64-mingw32-gcc
export CARGO_TARGET_X86_64_PC_WINDOWS_GNU_LINKER=x86_64-w64-mingw32-gcc
(
  cd "$ROOT"
  RUSTFLAGS="${RUSTFLAGS:--C target-feature=+crt-static}" \
    cargo build --release --target x86_64-pc-windows-gnu --bin flowplot
)

EXE="$ROOT/target/x86_64-pc-windows-gnu/release/flowplot.exe"
test -f "$EXE"

echo "==> stage install tree"
rm -rf "$STAGE"
mkdir -p "$STAGE/gui/assets"
cp -a "$EXE" "$STAGE/flowplot.exe"
cp -a "$WIN/README.txt" "$STAGE/README.txt"
cp -a "$WIN/DEPS.txt" "$STAGE/DEPS.txt"
cp -a "$WIN/flowplot-gui.bat" "$STAGE/flowplot-gui.bat"
cp -a "$WIN/add-to-path.bat" "$STAGE/add-to-path.bat"
cp -a "$ROOT/gui/flowplot_gui.py" "$STAGE/gui/flowplot_gui.py"
cp -a "$ROOT/gui/__init__.py" "$STAGE/gui/__init__.py"
cp -a "$ROOT/gui/assets/flowplot-wordmark.svg" "$STAGE/gui/assets/flowplot-wordmark.svg"

echo "==> wixl → $OUT"
mkdir -p "$(dirname "$OUT")"
(
  cd "$WIN"
  wixl -v -a x64 -o "$OUT" flowplot.wxs
)

ls -la "$OUT"
echo "OK: $OUT"
