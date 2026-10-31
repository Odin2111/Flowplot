# Fieldplot Agents menu (CLI / MCP)

User-facing **Agents** categories mapped onto `flowplot` CLI / MCP actions. Machine-readable twin: [`agents/postprocess-menu.yaml`](../agents/postprocess-menu.yaml). Full GUI guide: project-store `docs/agents-top-menu.md`.

Checks are **physics / numerics** on the loaded CFD file (any solver origin). Provenance fields in YAML may cite historical agent ids; the product surface does not require Lehar APIs.

```bash
flowplot menu                     # list categories + items
flowplot menu show <id>           # one item (template, status)
flowplot menu run <id> -- …       # dispatch ready items; document stubs
FLOWPLOT_BIN=… python3 agents/flowplot_mcp.py
```

## Categories

| Group | Role |
|-------|------|
| **Load & inspect** | Inventory zones/variables before analysis |
| **Positivity & sanity** | NaN/Inf/extrema; ρ/p/T positivity |
| **Profiles** | Centerline / line extracts; near-wall contours |
| **Reference values** | Freestream / length / time nondim refs |
| **Calculate variables** | Scalar contours (incl. turbulence names) |
| **Shocks** | θ–β–M polar + shock-evidence contours |
| **Hypersonic** | High-Mach extrema / Mach·p·ρ / bow-shock field |
| **Grid & mesh** | Zone inventory + mesh wireframe |
| **Geometry & boundaries** | Zone/boundary inventory + wall field |
| **Integration** | Nodal mean/extrema QoI summary |
| **Pathlines** | Velocity particle traces / overlays |
| **Suites** | Orchestrated evidence suites (mostly stub) |

## Status legend

| Status | Meaning |
|--------|---------|
| `ready` | `flowplot menu run <id> …` dispatches to a working Flowplot command |
| `stub` | Documents planned flags; does not invent a plot |

GUI: **Agents** top menu (replaces Analyze) → category cascades or **All Agents…**.
