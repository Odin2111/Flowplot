"""Ephemeral Fieldplot validation-data cache under ~/.fieldplot/validation_cache/.

Sources: ERCOFTAC (plotagent), NASA (nasaagent), arXiv (thin Atom fetch).
Users may Delete / Replace any case from the Validation data dialog.
"""

from __future__ import annotations

import json
import os
import shutil
from dataclasses import dataclass
from datetime import datetime, timezone
from pathlib import Path
from typing import Iterable

SOURCES = ("ercoftac", "nasa", "arxiv")


def default_cache_root() -> Path:
    env = os.environ.get("FIELDPLOT_VALIDATION_CACHE")
    if env:
        return Path(env).expanduser().resolve()
    return (Path.home() / ".fieldplot" / "validation_cache").resolve()


def ensure_cache_root(root: Path | None = None) -> Path:
    path = (root or default_cache_root()).resolve()
    path.mkdir(parents=True, exist_ok=True)
    for source in SOURCES:
        (path / source).mkdir(parents=True, exist_ok=True)
    index = path / "cache_index.json"
    if not index.is_file():
        write_index(path, {"version": 1, "cases": []})
    return path


def write_index(root: Path, payload: dict) -> None:
    root.mkdir(parents=True, exist_ok=True)
    (root / "cache_index.json").write_text(
        json.dumps(payload, indent=2) + "\n", encoding="utf-8"
    )


def read_index(root: Path) -> dict:
    path = root / "cache_index.json"
    if not path.is_file():
        return {"version": 1, "cases": []}
    try:
        return json.loads(path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError):
        return {"version": 1, "cases": []}


def dir_size_bytes(path: Path) -> int:
    if not path.exists():
        return 0
    total = 0
    if path.is_file():
        return path.stat().st_size
    for root, _dirs, files in os.walk(path):
        for name in files:
            try:
                total += (Path(root) / name).stat().st_size
            except OSError:
                pass
    return total


def format_size(n: int) -> str:
    if n < 1024:
        return f"{n} B"
    if n < 1024**2:
        return f"{n / 1024:.1f} KiB"
    if n < 1024**3:
        return f"{n / (1024**2):.1f} MiB"
    return f"{n / (1024**3):.2f} GiB"


def case_dir(root: Path, source: str, case_id: str) -> Path:
    return root / source / slugify(case_id)


def slugify(text: str) -> str:
    """Match nasaagent slugify so cache dirs resolve after download."""
    import re

    cleaned = re.sub(r"[^A-Za-z0-9._-]+", "_", text.strip())
    cleaned = re.sub(r"_+", "_", cleaned).strip("_.")
    return cleaned[:160] or "case"


def normalize_ercoftac_case(raw: str) -> str:
    text = raw.strip().lower().replace(" ", "")
    if text.startswith("case") and text[4:].isdigit():
        return f"case{int(text[4:]):03d}"
    if text.isdigit():
        return f"case{int(text):03d}"
    # allow "025" / "25"
    digits = "".join(ch for ch in text if ch.isdigit())
    if digits:
        return f"case{int(digits):03d}"
    raise ValueError(f"ERCOFTAC case id must look like case025 (got {raw!r})")


@dataclass
class CachedCase:
    source: str
    case_id: str
    path: Path
    size_bytes: int
    audit_path: Path | None
    modified: float

    @property
    def label(self) -> str:
        return f"{self.source}/{self.case_id}"


def list_cached_cases(root: Path | None = None) -> list[CachedCase]:
    base = ensure_cache_root(root)
    out: list[CachedCase] = []
    for source in SOURCES:
        src = base / source
        if not src.is_dir():
            continue
        for child in sorted(src.iterdir()):
            if not child.is_dir():
                continue
            audit = child / "audit.json"
            out.append(
                CachedCase(
                    source=source,
                    case_id=child.name,
                    path=child,
                    size_bytes=dir_size_bytes(child),
                    audit_path=audit if audit.is_file() else None,
                    modified=child.stat().st_mtime,
                )
            )
    return out


def delete_case(root: Path, source: str, case_id: str) -> bool:
    target = case_dir(root, source, case_id)
    if not target.exists():
        return False
    shutil.rmtree(target)
    refresh_index(root)
    return True


def purge_cache(root: Path | None = None) -> int:
    base = ensure_cache_root(root)
    freed = dir_size_bytes(base)
    for source in SOURCES:
        src = base / source
        if src.is_dir():
            shutil.rmtree(src)
            src.mkdir(parents=True, exist_ok=True)
    refresh_index(base)
    return freed


def refresh_index(root: Path) -> dict:
    cases = []
    for item in list_cached_cases(root):
        cases.append(
            {
                "source": item.source,
                "case_id": item.case_id,
                "path": str(item.path),
                "size_bytes": item.size_bytes,
                "audit": str(item.audit_path) if item.audit_path else None,
                "modified": datetime.fromtimestamp(
                    item.modified, tz=timezone.utc
                ).isoformat(),
            }
        )
    payload = {
        "version": 1,
        "root": str(root.resolve()),
        "updated": datetime.now(timezone.utc).isoformat(),
        "total_bytes": dir_size_bytes(root),
        "cases": cases,
    }
    write_index(root, payload)
    return payload


def find_data_tables(case_path: Path) -> list[Path]:
    """Prefer canonical CSVs, then any numeric table under the case tree."""
    preferred_dirs = ("canonical", "harmonized", "derived", ".")
    found: list[Path] = []
    seen: set[Path] = set()
    for dirname in preferred_dirs:
        base = case_path if dirname == "." else case_path / dirname
        if not base.is_dir():
            continue
        for path in sorted(base.rglob("*")):
            if not path.is_file():
                continue
            if path.suffix.lower() not in {".csv", ".dat", ".txt", ".tsv", ".xy"}:
                continue
            if path.name.lower() in {
                "normalization_audit.csv",
                "source_manifest.csv",
                "resource_manifest.csv",
                "applicability.csv",
                "ercoftac_catalog.csv",
                "all_cases_summary.csv",
            }:
                continue
            if path.name.lower().startswith("audit"):
                continue
            resolved = path.resolve()
            if resolved in seen:
                continue
            seen.add(resolved)
            found.append(path)
    return found


def read_table_columns(path: Path, max_rows: int = 8) -> tuple[list[str], list[list[str]]]:
    """Return header + preview rows from a CSV/DAT-like table."""
    text = path.read_text(encoding="utf-8", errors="replace")
    lines = [ln.strip() for ln in text.splitlines() if ln.strip() and not ln.strip().startswith("#")]
    if not lines:
        return [], []
    delim = "," if "," in lines[0] else ("\t" if "\t" in lines[0] else None)
    if delim is None:
        # whitespace
        parts0 = lines[0].split()
        header = _maybe_header(parts0)
        if header:
            rows = [ln.split() for ln in lines[1 : 1 + max_rows]]
            return header, rows
        header = [f"col{i+1}" for i in range(len(parts0))]
        rows = [ln.split() for ln in lines[:max_rows]]
        return header, rows
    parts0 = [p.strip() for p in lines[0].split(delim)]
    header = _maybe_header(parts0)
    if header:
        rows = [[p.strip() for p in ln.split(delim)] for ln in lines[1 : 1 + max_rows]]
        return header, rows
    header = [f"col{i+1}" for i in range(len(parts0))]
    rows = [[p.strip() for p in ln.split(delim)] for ln in lines[:max_rows]]
    return header, rows


def _maybe_header(parts: list[str]) -> list[str] | None:
    if not parts:
        return None
    non_numeric = 0
    for p in parts:
        try:
            float(p)
        except ValueError:
            non_numeric += 1
    if non_numeric >= max(1, len(parts) // 2):
        return [p or f"col{i+1}" for i, p in enumerate(parts)]
    return None


def load_xy_series(path: Path, x_col: str, y_col: str) -> tuple[list[float], list[float]]:
    header, _preview = read_table_columns(path, max_rows=1)
    text = path.read_text(encoding="utf-8", errors="replace")
    lines = [ln.strip() for ln in text.splitlines() if ln.strip() and not ln.strip().startswith("#")]
    if not lines:
        return [], []
    delim = "," if "," in lines[0] else ("\t" if "\t" in lines[0] else None)
    start = 0
    cols = header
    if delim is None:
        first = lines[0].split()
        if _maybe_header(first):
            cols = first
            start = 1
        else:
            cols = [f"col{i+1}" for i in range(len(first))]
    else:
        first = [p.strip() for p in lines[0].split(delim)]
        if _maybe_header(first):
            cols = first
            start = 1
        else:
            cols = [f"col{i+1}" for i in range(len(first))]
    try:
        ix = cols.index(x_col)
        iy = cols.index(y_col)
    except ValueError as exc:
        raise ValueError(f"columns {x_col!r}/{y_col!r} not in {cols}") from exc
    xs: list[float] = []
    ys: list[float] = []
    for ln in lines[start:]:
        parts = ln.split() if delim is None else [p.strip() for p in ln.split(delim)]
        if len(parts) <= max(ix, iy):
            continue
        try:
            xs.append(float(parts[ix]))
            ys.append(float(parts[iy]))
        except ValueError:
            continue
    return xs, ys


def load_sim_csv(path: Path) -> tuple[list[float], list[float]]:
    """Load Fieldplot line-sample / profile CSV → (s or x, value)."""
    text = path.read_text(encoding="utf-8", errors="replace")
    xs: list[float] = []
    ys: list[float] = []
    header_map: dict[str, int] | None = None
    for raw in text.splitlines():
        line = raw.strip()
        if not line or line.startswith("#"):
            continue
        parts = [p.strip() for p in line.split(",")]
        lower = [p.lower() for p in parts]
        if header_map is None and any(p.isalpha() or p in {"s", "x", "y", "value"} for p in lower):
            header_map = {name: i for i, name in enumerate(lower)}
            continue
        if header_map is None:
            # assume s,x,y,value or x,value
            if len(parts) >= 4:
                try:
                    xs.append(float(parts[0]))
                    ys.append(float(parts[3]))
                except ValueError:
                    continue
            elif len(parts) >= 2:
                try:
                    xs.append(float(parts[0]))
                    ys.append(float(parts[1]))
                except ValueError:
                    continue
            continue
        try:
            if "s" in header_map:
                x = float(parts[header_map["s"]])
            elif "x" in header_map:
                x = float(parts[header_map["x"]])
            else:
                x = float(parts[0])
            if "value" in header_map:
                y = float(parts[header_map["value"]])
            else:
                y = float(parts[-1])
        except (ValueError, IndexError):
            continue
        xs.append(x)
        ys.append(y)
    return xs, ys


def resolve_lehar_agents_root() -> Path | None:
    env = os.environ.get("LEHAR_AGENTS")
    candidates: list[Path] = []
    if env:
        candidates.append(Path(env).expanduser())
    candidates.extend(
        [
            Path.home() / "software" / "lehar-agents",
            Path("/home/nasu/software/lehar-agents"),
        ]
    )
    for path in candidates:
        if (path / "plot-agent").is_dir() or (path / "nasa-agent").is_dir():
            return path.resolve()
    return None


def plotagent_script(root: Path | None = None) -> Path | None:
    base = root or resolve_lehar_agents_root()
    if base is None:
        return None
    for rel in (
        Path("plot-agent") / "scripts" / "plotagent.py",
        Path("plot-agent") / "scripts" / "plotagent",
    ):
        p = base / rel
        if p.is_file():
            return p
    return None


def nasaagent_script(root: Path | None = None) -> Path | None:
    base = root or resolve_lehar_agents_root()
    if base is None:
        return None
    for rel in (
        Path("nasa-agent") / "nasaagent.py",
        Path("validation-agents") / "scripts" / "nasaagent.py",
        Path("hypersonic-data-agents") / "scripts" / "nasaagent.py",
    ):
        p = base / rel
        if p.is_file():
            return p
    return None


def iter_audit_snippets(audit_path: Path, limit: int = 40) -> Iterable[str]:
    try:
        data = json.loads(audit_path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as exc:
        yield f"(audit unreadable: {exc})"
        return
    yield json.dumps(data, indent=2)[:4000]
    _ = limit
