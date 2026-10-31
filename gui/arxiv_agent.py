"""Thin read-only arXiv Atom fetch for Fieldplot Validation data.

No dedicated lehar arXiv agent exists on DESKTOP; this mirrors nasaagent's
evidence-first pattern: download metadata (+ PDF when small), write audit.json,
never plot unless explicitly requested later by the Validation dialog.
"""

from __future__ import annotations

import argparse
import json
import re
import urllib.parse
import urllib.request
import xml.etree.ElementTree as ET
from datetime import datetime, timezone
from pathlib import Path

UA = "Fieldplot-ValidationData/0.1 (+read-only arXiv Atom)"
ATOM = "http://www.w3.org/2005/Atom"
ARXIV_API = "http://export.arxiv.org/api/query"


def _text(el: ET.Element | None) -> str:
    if el is None or el.text is None:
        return ""
    return " ".join(el.text.split())


def _local(tag: str) -> str:
    if "}" in tag:
        return tag.rsplit("}", 1)[-1]
    return tag


def search_arxiv(query: str, *, limit: int = 5, timeout: float = 45.0) -> list[dict]:
    """Return Atom entry dicts for an id_list or free-text search."""
    q = query.strip()
    # Bare arXiv id: 2301.12345 or arXiv:2301.12345v1
    id_match = re.fullmatch(
        r"(?:arxiv:)?(\d{4}\.\d{4,5})(v\d+)?", q, flags=re.IGNORECASE
    )
    params: dict[str, str]
    if id_match:
        params = {
            "id_list": id_match.group(1) + (id_match.group(2) or ""),
            "start": "0",
            "max_results": "1",
        }
    else:
        params = {
            "search_query": f"all:{q}",
            "start": "0",
            "max_results": str(max(1, min(limit, 25))),
            "sortBy": "relevance",
            "sortOrder": "descending",
        }
    url = f"{ARXIV_API}?{urllib.parse.urlencode(params)}"
    req = urllib.request.Request(url, headers={"User-Agent": UA})
    with urllib.request.urlopen(req, timeout=timeout) as resp:
        raw = resp.read()
    root = ET.fromstring(raw)
    entries: list[dict] = []
    for entry in root:
        if _local(entry.tag) != "entry":
            continue
        item: dict = {
            "id": "",
            "title": "",
            "summary": "",
            "published": "",
            "authors": [],
            "pdf_url": "",
            "abs_url": "",
        }
        for child in entry:
            name = _local(child.tag)
            if name == "id":
                item["abs_url"] = _text(child)
                m = re.search(r"arxiv\.org/abs/([^/\s]+)", item["abs_url"])
                item["id"] = m.group(1) if m else _text(child)
            elif name == "title":
                item["title"] = _text(child)
            elif name == "summary":
                item["summary"] = _text(child)
            elif name == "published":
                item["published"] = _text(child)
            elif name == "author":
                for nested in child:
                    if _local(nested.tag) == "name":
                        item["authors"].append(_text(nested))
            elif name == "link":
                href = child.attrib.get("href", "")
                title = child.attrib.get("title", "")
                typ = child.attrib.get("type", "")
                if title == "pdf" or typ == "application/pdf" or href.endswith(".pdf"):
                    item["pdf_url"] = href
        if item["id"]:
            entries.append(item)
    return entries


def download_bytes(url: str, *, timeout: float = 60.0, max_mb: float = 32.0) -> bytes:
    req = urllib.request.Request(url, headers={"User-Agent": UA})
    with urllib.request.urlopen(req, timeout=timeout) as resp:
        data = resp.read(int(max_mb * 1024 * 1024) + 1)
    if len(data) > max_mb * 1024 * 1024:
        raise RuntimeError(f"resource exceeds --max-download-mb ({max_mb})")
    return data


def run_arxiv_case(
    query: str,
    output_root: Path,
    *,
    refresh: bool = False,
    limit: int = 5,
    download_pdf: bool = True,
) -> Path:
    """Fetch top match into output_root/<id>/ with audit.json. Read-only (no plots)."""
    entries = search_arxiv(query, limit=limit)
    if not entries:
        raise RuntimeError(f"no arXiv hits for {query!r}")
    entry = entries[0]
    case_id = entry["id"].replace("/", "_")
    case_dir = output_root / case_id
    if case_dir.exists() and not refresh:
        audit = case_dir / "audit.json"
        if audit.is_file():
            return case_dir
    if case_dir.exists() and refresh:
        import shutil

        shutil.rmtree(case_dir)
    raw = case_dir / "raw"
    canonical = case_dir / "canonical"
    raw.mkdir(parents=True, exist_ok=True)
    canonical.mkdir(parents=True, exist_ok=True)

    meta_path = canonical / "metadata.json"
    meta_path.write_text(json.dumps(entry, indent=2) + "\n", encoding="utf-8")
    (canonical / "abstract.txt").write_text(entry.get("summary") or "", encoding="utf-8")
    # Lightweight series table: authors / year placeholders for series picker demos
    title_safe = (entry.get("title") or "arxiv").replace(",", " ")
    (canonical / "citation.csv").write_text(
        "field,value\n"
        f"arxiv_id,{entry['id']}\n"
        f"title,\"{title_safe}\"\n"
        f"published,{entry.get('published') or ''}\n"
        f"authors,\"{'; '.join(entry.get('authors') or [])}\"\n",
        encoding="utf-8",
    )

    pdf_path = None
    pdf_error = None
    if download_pdf and entry.get("pdf_url"):
        try:
            pdf_bytes = download_bytes(entry["pdf_url"])
            pdf_path = raw / f"{case_id}.pdf"
            pdf_path.write_bytes(pdf_bytes)
        except Exception as exc:  # noqa: BLE001 — record in audit
            pdf_error = str(exc)

    audit = {
        "agent": "arxiv-agent",
        "version": "0.1",
        "mode": "read-only",
        "plots_created": False,
        "query": query,
        "case_id": case_id,
        "title": entry.get("title"),
        "abs_url": entry.get("abs_url"),
        "pdf": str(pdf_path) if pdf_path else None,
        "pdf_error": pdf_error,
        "candidates": [
            {"id": e["id"], "title": e.get("title")} for e in entries[:limit]
        ],
        "fetched_at": datetime.now(timezone.utc).isoformat(),
        "status": "ok" if pdf_error is None or not download_pdf else "ok_partial",
    }
    (case_dir / "audit.json").write_text(json.dumps(audit, indent=2) + "\n", encoding="utf-8")
    return case_dir


def main(argv: list[str] | None = None) -> int:
    p = argparse.ArgumentParser(prog="arxiv-agent", description="Read-only arXiv fetch")
    p.add_argument("--case", required=True, help="arXiv id or search query")
    p.add_argument("--output", default="arxiv_output")
    p.add_argument("--refresh", action="store_true")
    p.add_argument("--limit", type=int, default=5)
    p.add_argument("--plots", action="store_true", help="ignored; plotting gated in Fieldplot")
    args = p.parse_args(argv)
    out = Path(args.output).expanduser().resolve()
    out.mkdir(parents=True, exist_ok=True)
    case_dir = run_arxiv_case(
        args.case, out, refresh=args.refresh, limit=args.limit, download_pdf=True
    )
    print(f"Output: {case_dir}")
    print(f"Audit: {case_dir / 'audit.json'}")
    if args.plots:
        print("Note: --plots ignored here; use Fieldplot Validation data → Plot compare")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
