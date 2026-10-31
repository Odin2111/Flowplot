#!/usr/bin/env bash
# Fieldplot (Flowplot GUI) launcher for WSL / WSLg.
# Prefers the agents worktree tip: cursor/hud-optical-agents-fix-e488
set -euo pipefail

TIP_BRANCH="${FIELDPLOT_TIP_BRANCH:-cursor/hud-optical-agents-fix-e488}"
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
  echo "Fieldplot tree not found. Tried: ${prefer_roots[*]}" >&2
  exit 1
fi

cd "$ROOT"
echo "Fieldplot root: $ROOT (DISPLAY=$DISPLAY)"

# Soft sync to tip when the tree is a git checkout and working tree is clean.
if [[ -d .git || -f .git ]] && command -v git >/dev/null 2>&1; then
  if [[ -z "$(git status --porcelain 2>/dev/null || true)" ]]; then
    git fetch origin "$TIP_BRANCH" >/dev/null 2>&1 || true
    if git show-ref --verify --quiet "refs/remotes/origin/$TIP_BRANCH"; then
      current="$(git rev-parse --abbrev-ref HEAD 2>/dev/null || echo unknown)"
      if [[ "$current" != "$TIP_BRANCH" ]]; then
        echo "Checking out tip branch $TIP_BRANCH (was $current)..."
        git checkout "$TIP_BRANCH" >/dev/null 2>&1 \
          || git checkout -B "$TIP_BRANCH" "origin/$TIP_BRANCH" >/dev/null 2>&1 \
          || echo "WARN: could not checkout $TIP_BRANCH; staying on $current" >&2
      fi
      git pull --ff-only origin "$TIP_BRANCH" >/dev/null 2>&1 \
        || echo "WARN: ff-only pull of $TIP_BRANCH skipped (local ahead or diverged)" >&2
    else
      echo "WARN: origin/$TIP_BRANCH not available; using current checkout" >&2
    fi
  else
    echo "WARN: dirty worktree — skipping checkout/pull; using current files in $ROOT" >&2
  fi
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
    echo "Building release flowplot (binary missing or stale)..."
    cargo build --release || echo "WARN: cargo build failed; launching GUI anyway" >&2
  else
    echo "WARN: cargo not found and $BIN missing/stale; launching GUI anyway" >&2
  fi
else
  echo "Release binary fresh: $BIN"
fi

exec python3 gui/flowplot_gui.py "$@"
