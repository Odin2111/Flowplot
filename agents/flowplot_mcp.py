#!/usr/bin/env python3
"""Minimal stdio MCP server wrapping the Flowplot CLI for lehar (or any) agents.

Tools mirror lehar-facing Flowplot commands:
  flowplot_load      — `flowplot load <path> --json`
  flowplot_contour   — `flowplot contour … --json`
  flowplot_nondim    — `flowplot nondim … --json`
  flowplot_pathlines — `flowplot pathlines … --json`
  flowplot_validate  — `flowplot validate … --json`
  flowplot_profile   — `flowplot profile … --json`
  flowplot_menu      — `flowplot menu [--group …] --json`
  flowplot_menu_run  — `flowplot menu run <id> -- … --json`

No third-party deps. Set FLOWPLOT_BIN to the `flowplot` executable (default: flowplot on PATH).
Protocol: JSON-RPC 2.0 MCP subset over newline-delimited stdin/stdout.
"""

from __future__ import annotations

import json
import os
import subprocess
import sys
from typing import Any

FLOWPLOT_BIN = os.environ.get("FLOWPLOT_BIN", "flowplot")


TOOLS = [
    {
        "name": "flowplot_load",
        "description": (
            "Load a CFD mesh/field file with Flowplot and return zone/variable JSON "
            "(Tecplot/VTK ASCII). Prefer this before contour/pathlines."
        ),
        "inputSchema": {
            "type": "object",
            "properties": {
                "path": {"type": "string", "description": "Path to .dat/.tp/.vtk (etc.)"},
            },
            "required": ["path"],
        },
    },
    {
        "name": "flowplot_contour",
        "description": "Render a contour PNG for a named scalar variable via Flowplot.",
        "inputSchema": {
            "type": "object",
            "properties": {
                "path": {"type": "string"},
                "var": {"type": "string"},
                "zone": {"type": "integer", "default": 0},
                "out": {"type": "string", "default": "contour.png"},
                "plane": {"type": "string", "description": "e.g. k=0"},
                "vmin": {"type": "number"},
                "vmax": {"type": "number"},
                "u_ref": {"type": "number"},
                "lx": {"type": "number"},
                "ly": {"type": "number"},
                "lz": {"type": "number"},
                "t_ref": {"type": "number"},
                "pathlines": {"type": "boolean", "default": False},
                "pathlines_out": {"type": "string"},
                "seed": {"type": "string", "description": "x,y seed (repeat via seeds)"},
                "seeds": {
                    "type": "array",
                    "items": {"type": "string"},
                    "description": "list of x,y seeds",
                },
                "u_var": {"type": "string"},
                "v_var": {"type": "string"},
            },
            "required": ["path", "var"],
        },
    },
    {
        "name": "flowplot_nondim",
        "description": (
            "Ask Flowplot for name-based nondimensionalization suggestions and optionally "
            "apply user-supplied scales (never invents physics values)."
        ),
        "inputSchema": {
            "type": "object",
            "properties": {
                "path": {"type": "string"},
                "u_ref": {"type": "number"},
                "lx": {"type": "number"},
                "ly": {"type": "number"},
                "lz": {"type": "number"},
                "t_ref": {"type": "number"},
                "x_var": {"type": "string"},
                "y_var": {"type": "string"},
                "z_var": {"type": "string"},
            },
            "required": ["path"],
        },
    },
    {
        "name": "flowplot_pathlines",
        "description": "Trace U/V pathlines and write a PNG via Flowplot.",
        "inputSchema": {
            "type": "object",
            "properties": {
                "path": {"type": "string"},
                "out": {"type": "string", "default": "pathlines.png"},
                "zone": {"type": "integer", "default": 0},
                "var": {"type": "string", "description": "optional underlay contour variable"},
                "seed": {"type": "string"},
                "seeds": {"type": "array", "items": {"type": "string"}},
                "u_var": {"type": "string"},
                "v_var": {"type": "string"},
                "w_var": {"type": "string"},
                "dt": {"type": "number"},
                "u_ref": {"type": "number"},
                "lx": {"type": "number"},
                "ly": {"type": "number"},
                "lz": {"type": "number"},
                "t_ref": {"type": "number"},
            },
            "required": ["path"],
        },
    },
    {
        "name": "flowplot_validate",
        "description": (
            "Field sanity scan (NaN/Inf/extrema); optional positivity on ρ/p/T names."
        ),
        "inputSchema": {
            "type": "object",
            "properties": {
                "path": {"type": "string"},
                "positivity": {"type": "boolean", "default": False},
                "zone": {"type": "integer"},
            },
            "required": ["path"],
        },
    },
    {
        "name": "flowplot_profile",
        "description": (
            "Extract a 1-D profile / centerline; optional RMSE vs a two-column ref CSV."
        ),
        "inputSchema": {
            "type": "object",
            "properties": {
                "path": {"type": "string"},
                "var": {"type": "string"},
                "line": {"type": "string", "default": "mid-j"},
                "zone": {"type": "integer", "default": 0},
                "plane": {"type": "string"},
                "ref": {"type": "string", "description": "Optional reference CSV path"},
                "out": {"type": "string"},
            },
            "required": ["path", "var"],
        },
    },
    {
        "name": "flowplot_menu",
        "description": (
            "List the CFD post-processing menu (groups/items from "
            "agents/postprocess-menu.yaml) as JSON."
        ),
        "inputSchema": {
            "type": "object",
            "properties": {
                "group": {
                    "type": "string",
                    "description": "Optional group id filter (e.g. shocks, suites).",
                },
                "id": {
                    "type": "string",
                    "description": "If set, show one menu item instead of listing.",
                },
            },
        },
    },
    {
        "name": "flowplot_menu_run",
        "description": (
            "Run a menu item id. Ready items dispatch to Flowplot; stubs return "
            "requires-agent documentation and planned flags."
        ),
        "inputSchema": {
            "type": "object",
            "properties": {
                "id": {"type": "string", "description": "Menu item id (e.g. contour, bow-shock-field)"},
                "args": {
                    "type": "array",
                    "items": {"type": "string"},
                    "description": "Args forwarded after `flowplot <command>` (path, --var, -o, …).",
                },
            },
            "required": ["id"],
        },
    },
]


def run_flowplot(argv: list[str]) -> dict[str, Any]:
    cmd = [FLOWPLOT_BIN, *argv]
    try:
        proc = subprocess.run(
            cmd,
            capture_output=True,
            text=True,
            check=False,
        )
    except FileNotFoundError:
        return {
            "status": "error",
            "error": f"flowplot binary not found: {FLOWPLOT_BIN!r}; set FLOWPLOT_BIN",
            "cmd": cmd,
        }
    stdout = (proc.stdout or "").strip()
    stderr = (proc.stderr or "").strip()
    payload: dict[str, Any]
    try:
        payload = json.loads(stdout) if stdout else {}
    except json.JSONDecodeError:
        payload = {"status": "error", "raw_stdout": stdout}
    payload.setdefault("exit_code", proc.returncode)
    if stderr:
        payload["stderr"] = stderr
    payload["cmd"] = cmd
    return payload


def _seed_args(arguments: dict[str, Any]) -> list[str]:
    argv: list[str] = []
    seeds = list(arguments.get("seeds") or [])
    if arguments.get("seed"):
        seeds.append(str(arguments["seed"]))
    for seed in seeds:
        argv.extend(["--seed", str(seed)])
    return argv


def _nondim_args(arguments: dict[str, Any]) -> list[str]:
    argv: list[str] = []
    for flag, key in (
        ("--u-ref", "u_ref"),
        ("--lx", "lx"),
        ("--ly", "ly"),
        ("--lz", "lz"),
        ("--t-ref", "t_ref"),
        ("--x-var", "x_var"),
        ("--y-var", "y_var"),
        ("--z-var", "z_var"),
    ):
        if arguments.get(key) is not None:
            argv.extend([flag, str(arguments[key])])
    return argv


def call_tool(name: str, arguments: dict[str, Any]) -> dict[str, Any]:
    if name == "flowplot_load":
        return run_flowplot(["load", str(arguments["path"]), "--json"])

    if name == "flowplot_contour":
        argv = [
            "contour",
            str(arguments["path"]),
            "--var",
            str(arguments["var"]),
            "--zone",
            str(arguments.get("zone", 0)),
            "--out",
            str(arguments.get("out", "contour.png")),
        ]
        if arguments.get("plane"):
            argv.extend(["--plane", str(arguments["plane"])])
        if arguments.get("vmin") is not None:
            argv.extend(["--vmin", str(arguments["vmin"])])
        if arguments.get("vmax") is not None:
            argv.extend(["--vmax", str(arguments["vmax"])])
        argv.extend(_nondim_args(arguments))
        if arguments.get("pathlines"):
            argv.append("--pathlines")
        if arguments.get("pathlines_out"):
            argv.extend(["--pathlines-out", str(arguments["pathlines_out"])])
        if arguments.get("u_var"):
            argv.extend(["--u-var", str(arguments["u_var"])])
        if arguments.get("v_var"):
            argv.extend(["--v-var", str(arguments["v_var"])])
        argv.extend(_seed_args(arguments))
        argv.append("--json")
        return run_flowplot(argv)

    if name == "flowplot_nondim":
        argv = ["nondim", str(arguments["path"]), *_nondim_args(arguments), "--json"]
        return run_flowplot(argv)

    if name == "flowplot_pathlines":
        argv = [
            "pathlines",
            str(arguments["path"]),
            "--out",
            str(arguments.get("out", "pathlines.png")),
            "--zone",
            str(arguments.get("zone", 0)),
        ]
        if arguments.get("var"):
            argv.extend(["--var", str(arguments["var"])])
        if arguments.get("u_var"):
            argv.extend(["--u-var", str(arguments["u_var"])])
        if arguments.get("v_var"):
            argv.extend(["--v-var", str(arguments["v_var"])])
        if arguments.get("w_var"):
            argv.extend(["--w-var", str(arguments["w_var"])])
        if arguments.get("dt") is not None:
            argv.extend(["--dt", str(arguments["dt"])])
        argv.extend(_seed_args(arguments))
        argv.extend(_nondim_args(arguments))
        argv.append("--json")
        return run_flowplot(argv)

    if name == "flowplot_validate":
        argv = ["validate", str(arguments["path"])]
        if arguments.get("positivity"):
            argv.append("--positivity")
        if arguments.get("zone") is not None:
            argv.extend(["--zone", str(arguments["zone"])])
        argv.append("--json")
        return run_flowplot(argv)

    if name == "flowplot_profile":
        argv = [
            "profile",
            str(arguments["path"]),
            "--var",
            str(arguments["var"]),
            "--line",
            str(arguments.get("line", "mid-j")),
            "--zone",
            str(arguments.get("zone", 0)),
        ]
        if arguments.get("plane"):
            argv.extend(["--plane", str(arguments["plane"])])
        if arguments.get("ref"):
            argv.extend(["--ref", str(arguments["ref"])])
        if arguments.get("out"):
            argv.extend(["-o", str(arguments["out"])])
        argv.append("--json")
        return run_flowplot(argv)

    if name == "flowplot_menu":
        if arguments.get("id"):
            return run_flowplot(["--json", "menu", "show", str(arguments["id"])])
        argv = ["--json", "menu", "list"]
        if arguments.get("group"):
            argv.extend(["--group", str(arguments["group"])])
        return run_flowplot(argv)

    if name == "flowplot_menu_run":
        argv = ["--json", "menu", "run", str(arguments["id"]), "--"]
        for a in arguments.get("args") or []:
            argv.append(str(a))
        return run_flowplot(argv)

    return {"status": "error", "error": f"unknown tool: {name}"}


def respond(msg_id: Any, result: Any = None, error: dict[str, Any] | None = None) -> None:
    body: dict[str, Any] = {"jsonrpc": "2.0", "id": msg_id}
    if error is not None:
        body["error"] = error
    else:
        body["result"] = result
    sys.stdout.write(json.dumps(body) + "\n")
    sys.stdout.flush()


def handle(msg: dict[str, Any]) -> None:
    method = msg.get("method")
    msg_id = msg.get("id")
    params = msg.get("params") or {}

    if method == "initialize":
        respond(
            msg_id,
            {
                "protocolVersion": "2024-11-05",
                "capabilities": {"tools": {}},
                "serverInfo": {"name": "flowplot", "version": "0.1.0"},
            },
        )
        return
    if method == "notifications/initialized":
        return
    if method == "tools/list":
        respond(msg_id, {"tools": TOOLS})
        return
    if method == "tools/call":
        name = params.get("name", "")
        arguments = params.get("arguments") or {}
        payload = call_tool(name, arguments)
        text = json.dumps(payload, indent=2)
        is_error = payload.get("status") == "error" or int(payload.get("exit_code") or 0) != 0
        respond(
            msg_id,
            {
                "content": [{"type": "text", "text": text}],
                "isError": is_error,
            },
        )
        return
    if method == "ping":
        respond(msg_id, {})
        return
    if msg_id is not None:
        respond(msg_id, error={"code": -32601, "message": f"Method not found: {method}"})


def main() -> None:
    for line in sys.stdin:
        line = line.strip()
        if not line:
            continue
        try:
            msg = json.loads(line)
        except json.JSONDecodeError as exc:
            respond(None, error={"code": -32700, "message": f"Parse error: {exc}"})
            continue
        handle(msg)


if __name__ == "__main__":
    main()
