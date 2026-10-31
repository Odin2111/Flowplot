#!/usr/bin/env bash
# Fieldplot (Flowplot GUI) launcher for WSL / WSLg.
# Desktop Fieldplot.bat / .lnk must call this script by absolute path — no nested
# bash -lc quoting (cmd.exe mangles \"$ROOT\" and leaves ROOT empty).
set -euo pipefail

LOG="${FIELDPLOT_LAUNCH_LOG:-/tmp/fieldplot-launch.log}"
{
  echo "==== $(date -Is) pid=$$ ===="
  echo "argv: $*"
  echo "DISPLAY=${DISPLAY:-}"
  echo "WAYLAND_DISPLAY=${WAYLAND_DISPLAY:-}"
  echo "PWD=$PWD"
  echo "USER=${USER:-}"
} >>"$LOG" 2>&1 || true

# Prefer the stacked tip; allow override.
TIP_BRANCH="${FIELDPLOT_TIP_BRANCH:-cursor/contour-var-desktop-launch-207d}"
# Fallbacks when the preferred tip is not on origin yet.
TIP_FALLBACKS=(
  "$TIP_BRANCH"
  cursor/desktop-click-blank-load-8052
  cursor/fieldplot-desktop-launcher-ac9a
  cursor/hud-optical-agents-fix-e488
)

DISPLAY_VAL="${DISPLAY:-:0}"
export DISPLAY="$DISPLAY_VAL"

prefer_roots=(
  "${FIELDPLOT_ROOT:-}"
  "$HOME/software/Flowplot-agents-wt"
  "$HOME/software/Flowplot"
)

ROOT=""
for cand in "${prefer_roots[@]}"; do
  [[ -z "$cand" ]] && continue
  if [[ -d "$cand" && -f "$cand/gui/flowplot_gui.py" ]]; then
    ROOT="$cand"
    break
  fi
done

if [[ -z "$ROOT" ]]; then
  echo "Fieldplot tree not found. Tried: ${prefer_roots[*]}" | tee -a "$LOG" >&2
  exit 1
fi

cd "$ROOT"
# gui/*.py use bare imports (agents_panel, …) — require gui/ on sys.path.
export PYTHONPATH="$ROOT/gui${PYTHONPATH:+:$PYTHONPATH}"

echo "Fieldplot root: $ROOT (DISPLAY=$DISPLAY tip=$TIP_BRANCH)" | tee -a "$LOG"

# Soft sync to tip when clean. Desktop launches set FIELDPLOT_SKIP_GIT_SYNC=1 so
# a hung/slow fetch cannot block double-click. Timeout every git network call.
if [[ "${FIELDPLOT_SKIP_GIT_SYNC:-0}" != "1" ]] \
  && { [[ -d .git ]] || [[ -f .git ]]; } \
  && command -v git >/dev/null 2>&1; then
  if [[ -z "$(git status --porcelain 2>/dev/null || true)" ]]; then
    synced=0
    for tip in "${TIP_FALLBACKS[@]}"; do
      timeout 20 git fetch origin "$tip" >/dev/null 2>&1 || true
      if git show-ref --verify --quiet "refs/remotes/origin/$tip"; then
        current="$(git rev-parse --abbrev-ref HEAD 2>/dev/null || echo unknown)"
        if [[ "$current" != "$tip" ]]; then
          echo "Checking out tip branch $tip (was $current)..." | tee -a "$LOG"
          git checkout "$tip" >/dev/null 2>&1 \
            || git checkout -B "$tip" "origin/$tip" >/dev/null 2>&1 \
            || { echo "WARN: could not checkout $tip; trying next" | tee -a "$LOG" >&2; continue; }
        fi
        timeout 20 git pull --ff-only origin "$tip" >/dev/null 2>&1 \
          || echo "WARN: ff-only pull of $tip skipped (local ahead or diverged)" | tee -a "$LOG" >&2
        synced=1
        break
      fi
    done
    if [[ "$synced" -eq 0 ]]; then
      echo "WARN: no tip branch on origin; using current checkout" | tee -a "$LOG" >&2
    fi
  else
    echo "WARN: dirty worktree — skipping checkout/pull; using current files in $ROOT" | tee -a "$LOG" >&2
  fi
else
  echo "Git tip sync skipped (FIELDPLOT_SKIP_GIT_SYNC=${FIELDPLOT_SKIP_GIT_SYNC:-0})" | tee -a "$LOG"
fi

BIN="target/release/flowplot"
need_build=0
if [[ ! -x "$BIN" ]]; then
  need_build=1
elif [[ -f Cargo.toml && "$BIN" -ot Cargo.toml ]]; then
  need_build=1
elif [[ -d src ]] && find src -type f -newer "$BIN" 2>/dev/null | grep -q .; then
  need_build=1
fi

if [[ "$need_build" -eq 1 ]]; then
  if command -v cargo >/dev/null 2>&1; then
    echo "Building release flowplot (binary missing or stale)..." | tee -a "$LOG"
    cargo build --release || echo "WARN: cargo build failed; launching GUI anyway" | tee -a "$LOG" >&2
  else
    echo "WARN: cargo not found and $BIN missing/stale; launching GUI anyway" | tee -a "$LOG" >&2
  fi
else
  echo "Release binary fresh: $BIN" | tee -a "$LOG"
fi

echo "exec python3 gui/flowplot_gui.py $*" | tee -a "$LOG"
# Keep python stderr in the launch log (exec replaces this shell).
exec >>"$LOG" 2>&1
exec python3 gui/flowplot_gui.py "$@"
