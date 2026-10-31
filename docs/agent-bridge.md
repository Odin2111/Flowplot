# Agent bridge (lehar ↔ Flowplot)

`lehar-agent` (and sibling plot skills) should drive Flowplot through the **CLI JSON contract** or the thin **stdio MCP** wrapper. Do not call external proprietary GUI/plot APIs for ASCII cases Flowplot already reads, and do not invent proprietary binary readers.

## CLI (stable agent contract)

```bash
cargo build --release
export FLOWPLOT_BIN="$(pwd)/target/release/flowplot"

"$FLOWPLOT_BIN" load <path> --json
"$FLOWPLOT_BIN" contour <path> --var NAME -o out.png [--plane k=0] [--u-ref …] [--pathlines] --json
"$FLOWPLOT_BIN" nondim <path> [--u-ref U] [--lx Lx] [--ly Ly] [--lz Lz] [--t-ref T] --json
"$FLOWPLOT_BIN" pathlines <path> -o lines.png [--seed x,y] [--u-var U] [--v-var V] [--var P] --json
"$FLOWPLOT_BIN" menu --json
"$FLOWPLOT_BIN" menu run <id> -- <flowplot-args…> --json
```

Post-processing menu (lehar agent ↔ Flowplot): [`docs/postprocess-menu.md`](postprocess-menu.md), machine-readable [`agents/postprocess-menu.yaml`](../agents/postprocess-menu.yaml).

Bare `flowplot <path> --json` is an alias of `load`. Every successful JSON payload has `"status":"ok"`; failures use `"status":"error"` and a non-zero exit code.

`nondim` only **suggests** scales from variable names unless the caller passes `--u-ref` / `--lx` / … — it never invents physics values.

## MCP

```bash
FLOWPLOT_BIN=./target/release/flowplot python3 agents/flowplot_mcp.py
```

Tools: `flowplot_load`, `flowplot_contour`, `flowplot_nondim`, `flowplot_pathlines`, `flowplot_menu`, `flowplot_menu_run`. Newline-delimited JSON-RPC on stdin/stdout.

## Match to lehar-agent interfaces

| lehar surface | Flowplot call |
|---|---|
| `lehar-agent data inspect` (ASCII `.dat`/`.tec` discovery) | `flowplot load <file> --json` for zone/variable inventory |
| Field / research figures (`plt-animation-ascii`, `research_figures`, `observability_plots`) | `contour` / `pathlines` PNG artifacts under the run `engineering/` or `plots/` tree |
| Case nondim / ERCOFTAC-style audits (`plot-agent`, BL agents) | `nondim --json` then pass the same scales into `contour` / `pathlines` |
| FastAPI `POST /api/v1/runs/{run_id}/engineering/{agent_id}` | Agent implementation shells CLI or MCP; returns `AgentArtifact.plots` paths |
| `artifact export-plot` | Prefer Flowplot PNG when the source is public PLT/VTK ASCII |

Binary `.szplt` and other proprietary packs stay on lehar/VisIt adapters — Flowplot reads public `.plt` layouts only and does not reverse-engineer closed formats.

## Suggested agent workflow

1. Discover solution ASCII under the run → `load --json`.
2. If variables look dimensional → `nondim --json`; apply user/case scales only.
3. Contour QoI → `contour --var … -o … --json`.
4. Optional topology → `pathlines … --json` or `contour --pathlines`.
5. Attach PNG paths on the engineering artifact / FastAPI response.
