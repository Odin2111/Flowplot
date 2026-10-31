# CFD post-processing menu

User-facing menu of **useful lehar-agent CFD post-processing workflows** mapped onto Flowplot CLI / MCP actions. Machine-readable twin: [`agents/postprocess-menu.yaml`](../agents/postprocess-menu.yaml).

```bash
flowplot menu                     # list groups + items
flowplot menu show <id>           # one item (flags, lehar agent, status)
flowplot menu run <id> -- …       # dispatch ready items; document stubs
FLOWPLOT_BIN=… python3 agents/flowplot_mcp.py   # tools flowplot_menu*
```

Binary `.plt` / `.szplt` stays on lehar native readers. Flowplot only takes public Tecplot/VTK ASCII.

## Groups

| Group | Role |
|-------|------|
| **Load & inspect** | Inventory zones/variables before plotting |
| **Validation & correction** | Field sanity, positivity, profile extracts on the loaded file |
| **Contours** | Scalar pseudocolor PNGs |
| **Nondim** | Name-based scale suggestions; apply user/case refs only |
| **Pathlines** | Velocity particle traces / overlays |
| **Shocks** | Shock-aware field figures (jump math stays in lehar) |
| **Figures / animation** | Research figures and ASCII frame sequences |
| **Suites** | Orchestrated evidence suites attaching field panels |

## Status legend

| Status | Meaning |
|--------|---------|
| `ready` | `flowplot menu run <id> …` dispatches to a working Flowplot command |
| `stub` | Documents the lehar agent + planned Flowplot flags; does not invent a plot |

---

### Load & inspect

| id | Title | Status | Lehar | Flowplot / MCP |
|----|-------|--------|-------|----------------|
| `load` | Load dataset | ready | `data inspect` | `flowplot load {path} --json` → `flowplot_load` |
| `info` | Dataset info | ready | `data inspect` | `flowplot info {path} --json` → `flowplot_load` |
| `output-integrity` | Output integrity inventory | stub | `output_integrity` | discover paths in lehar, then `load --json` |

### Validation & correction

| id | Title | Status | Lehar | Flowplot / MCP |
|----|-------|--------|-------|----------------|
| `field-sanity` | Field sanity (NaN / Inf / extrema) | ready | `positivity_health` / validation | `validate --json` |
| `positivity-check` | Positivity check (ρ / p / T) | ready | `positivity_health` | `validate --positivity --json` |
| `extract-profile` | Extract centerline / profile | ready | `data extract-line` | `profile --var … [--ref …] --json` |

### Contours

| id | Title | Status | Lehar | Flowplot / MCP |
|----|-------|--------|-------|----------------|
| `contour` | Contour scalar | ready | `research_figures` / `observability_plots` | `contour --var … -o …` → `flowplot_contour` |
| `wall-physics-field` | Wall-physics near-wall field | stub | `wall_physics` | `contour --var … -o …` |
| `turbulence-field` | Turbulence field contour | stub | `turbulence_health` | `contour --var k\|omega\|nut …` |
| `positivity-field` | Positivity scalar contour | ready | `positivity_health` | `contour --var rho\|p\|T …` |
| `mesh-solution-field` | Mesh–solution QoI contour | stub | `mesh_solution_correlation` | `contour --var …` |

### Nondim

| id | Title | Status | Lehar | Flowplot / MCP |
|----|-------|--------|-------|----------------|
| `nondim-suggest` | Suggest / apply scales | ready | plot-agent / BL | `nondim [--u-ref …]` → `flowplot_nondim` |
| `boundary-layer-scales` | Boundary-layer reference scales | stub | `boundary_layer` | `nondim --u-ref U_e --lx δ*` |
| `plot-agent-scales` | Plot-agent / ERCOFTAC scales | ready | `plot-agent` | `nondim [--u-ref …] [--lx …]` |

### Pathlines

| id | Title | Status | Lehar | Flowplot / MCP |
|----|-------|--------|-------|----------------|
| `pathlines` | Trace pathlines | ready | `research_figures` | `pathlines -o …` → `flowplot_pathlines` |
| `contour-pathlines` | Contour with pathline overlay | ready | `research_figures` | `contour … --pathlines` → `flowplot_contour` |
| `interface-growth-field` | Interface-growth density + pathlines | stub | `interface_growth` | `contour --var Density --pathlines` |

### Shocks

| id | Title | Status | Lehar | Flowplot / MCP |
|----|-------|--------|-------|----------------|
| `bow-shock-field` | Bow-shock field | stub | `bow_shock_validation` | `load` → `nondim` → `contour` (Mach/ρ); optional `--pathlines` |
| `shock-polar-field` | Shock polar (θ–β–M) | ready | `shock_polar` | Analyze chart (`gui/shock_polar.py`); CFD polar in lehar |
| `shock-tracking-field` | Shock-tracking field evidence | stub | `shock_tracking` | `contour --var p\|rho` |
| `shock-stability-field` | Shock-sensor / pressure contour | stub | `shock_stability` | `contour --var sensor\|p` |
| `rankine-hugoniot-field` | Rankine–Hugoniot state contour | stub | `rankine_hugoniot` | `contour` optional evidence |

### Figures / animation

| id | Title | Status | Lehar | Flowplot / MCP |
|----|-------|--------|-------|----------------|
| `research-figures` | Research field figures | stub | `research_figures` | `load` → `nondim` → `contour`/`pathlines` |
| `tecplot-animation-ascii` | ASCII frame animation | stub | `tecplot_animation` | per-frame `contour`; binary `.plt` stays lehar |
| `spod-mode-field` | SPOD mode amplitude field | stub | `spod_analysis` / `postprocess spod` | `contour` if modes exported as field |

### Suites

| id | Title | Status | Lehar | Flowplot / MCP |
|----|-------|--------|-------|----------------|
| `rans-suite` | RANS evidence suite | stub | `rans_suite` | child agents + `contour` panels |
| `les-suite` | LES evidence suite | stub | `les_suite` | same |
| `dns-suite` | DNS evidence suite | stub | `dns_suite` | same |
| `hybrid-rans-les-suite` | Hybrid RANS–LES suite | stub | `hybrid_rans_les_suite` | same |
| `hypersonic-euler-suite` | Hypersonic Euler suite | stub | `hypersonic_euler_suite` | `contour` Mach/p/ρ; optional `--pathlines` |
| `hypersonic-viscous-suite` | Hypersonic viscous suite | stub | `hypersonic_viscous_suite` | same + wall `--plane` |

## Ready recipes

```bash
flowplot menu run load -- tests/fixtures/simple.dat --json
flowplot menu run contour -- tests/fixtures/flow.dat --var P -o /tmp/p.png --json
flowplot menu run nondim-suggest -- tests/fixtures/flow.dat --json
flowplot menu run pathlines -- tests/fixtures/flow.dat -o /tmp/lines.png --json
flowplot menu run contour-pathlines -- tests/fixtures/flow.dat --var P -o /tmp/p.png --pathlines --json
```

Stub example:

```text
$ flowplot menu run bow-shock-field
stub: requires agent bow_shock_validation
flowplot: flowplot load {path} --json && …
planned flags: --var -o --u-ref --lx --pathlines --json
```

## See also

- [`docs/agent-bridge.md`](agent-bridge.md) — CLI JSON / MCP contract
- lehar inventory note (project store) — full engineering-agent ↔ Flowplot map
