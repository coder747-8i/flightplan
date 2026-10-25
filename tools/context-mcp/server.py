#!/usr/bin/env python3
"""flightplan-context: a local MCP server that keeps a cached map of this repository so agents
can find their way around without rereading whole files.

It speaks MCP (JSON-RPC 2.0, one message per line) over stdio and needs only the Python 3.10+
standard library. The index (per-file module docs, symbols with line numbers and signatures)
lives in .cache/context-mcp/index.json and is refreshed incrementally: on every call, files whose
size or mtime changed are re-parsed, so answers always match the working tree.

Curated knowledge lives in docs/context/: OVERVIEW.md (architecture, start here) and NOTES.md
(gotchas and decisions agents add with the `add_note` tool). Both are committed, so the next
agent inherits them.

Run by hand to check it:  python3 tools/context-mcp/server.py --selftest
"""

from __future__ import annotations

import fnmatch
import hashlib
import json
import os
import re
import subprocess
import sys
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
CACHE = ROOT / ".cache" / "context-mcp" / "index.json"
DOCS = ROOT / "docs" / "context"
OVERVIEW = DOCS / "OVERVIEW.md"
NOTES = DOCS / "NOTES.md"
INDEX_VERSION = 3
PROTOCOL = "2025-06-18"

SOURCE_EXT = {".rs", ".ts", ".svelte", ".py", ".sh", ".js", ".md", ".toml", ".yml", ".yaml", ".css", ".html"}
SKIP_PARTS = {"node_modules", "target", ".git", ".venv", ".cache", "dist", ".tools"}
MAX_FILE = 600_000


# ----------------------------------------------------------------------------------------------
# indexing


def list_files() -> list[str]:
    """Tracked and untracked-but-not-ignored files, relative to ROOT."""
    try:
        out = subprocess.run(["git", "ls-files", "-co", "--exclude-standard"], cwd=ROOT, capture_output=True, text=True, timeout=30).stdout
        files = [f for f in out.splitlines() if f]
    except Exception:
        files = [str(p.relative_to(ROOT)) for p in ROOT.rglob("*") if p.is_file()]
    return sorted(f for f in files if Path(f).suffix in SOURCE_EXT and not (set(Path(f).parts) & SKIP_PARTS))


RUST_ITEM = re.compile(r"^(\s*)(?:pub(?:\([^)]*\))?\s+)?(?:async\s+)?(?:unsafe\s+)?(?:extern\s+\"C\"\s+)?(fn|struct|enum|trait|impl|mod|const|static|type|macro_rules!)\s*([A-Za-z_][A-Za-z0-9_]*)?")
TS_ITEM = re.compile(r"^(\s*)(?:export\s+)?(?:default\s+)?(?:async\s+)?(function\*?|class|interface|type|const|let|enum)\s+([A-Za-z_$][A-Za-z0-9_$]*)")
PY_ITEM = re.compile(r"^(\s*)(?:async\s+)?(def|class)\s+([A-Za-z_][A-Za-z0-9_]*)")
SH_ITEM = re.compile(r"^()(?:function\s+)?([A-Za-z_][A-Za-z0-9_]*)\s*\(\)\s*\{")
MD_ITEM = re.compile(r"^(#{1,3})\s+(.*)")


def module_doc(path: str, lines: list[str]) -> str:
    """The file's own description: //! lines, a leading docstring/comment block, or the first heading."""
    ext = Path(path).suffix
    doc: list[str] = []
    if ext == ".rs":
        for l in lines:
            s = l.strip()
            if s.startswith("//!"):
                doc.append(s[3:].strip())
            elif s and not s.startswith("#!") and not s.startswith("//"):
                break
    elif ext == ".py":
        text = "\n".join(lines[:40])
        m = re.search(r'^(?:#![^\n]*\n)?\s*(?:"""|\'\'\')(.*?)(?:"""|\'\'\')', text, re.S)
        if m:
            doc = [x.strip() for x in m.group(1).strip().splitlines()]
    elif ext in {".sh", ".yml", ".yaml", ".toml"}:
        for l in lines[:30]:
            s = l.strip()
            if s.startswith("#!"):
                continue
            if s.startswith("#"):
                doc.append(s.lstrip("#").strip())
            elif s:
                break
    elif ext in {".ts", ".js", ".svelte"}:
        body = lines
        if ext == ".svelte":
            body = [l for l in lines if not l.strip().startswith("<script")]
        for l in body[:30]:
            s = l.strip()
            if s.startswith("//") or s.startswith("/*") or s.startswith("*"):
                doc.append(s.lstrip("/*").strip().rstrip("*/").strip())
            elif s:
                break
    elif ext == ".md":
        for l in lines:
            if l.startswith("#"):
                return l.lstrip("#").strip()
    return " ".join(d for d in doc if d)[:600]


def preceding_doc(lines: list[str], i: int) -> str:
    """Doc comment lines (/// or # or //) just above line i."""
    out: list[str] = []
    j = i - 1
    while j >= 0:
        s = lines[j].strip()
        if s.startswith("///"):
            out.append(s[3:].strip())
        elif s.startswith("#[") or s.startswith("@"):
            pass
        elif s.startswith("//") and not s.startswith("//!"):
            out.append(s[2:].strip())
        else:
            break
        j -= 1
    return " ".join(reversed(out))[:300]


def outline_of(path: str, lines: list[str]) -> list[dict]:
    ext = Path(path).suffix
    items: list[dict] = []
    in_script = ext != ".svelte"
    for i, l in enumerate(lines):
        if ext == ".svelte":
            if "<script" in l:
                in_script = True
                continue
            if "</script>" in l:
                in_script = False
            if not in_script:
                continue
        m = None
        if ext == ".rs":
            m = RUST_ITEM.match(l)
            if m:
                kind, name = m.group(2), m.group(3) or ""
                if kind == "impl":
                    name = l.strip().rstrip("{").strip()
                if len(m.group(1)) > 8 and kind not in ("fn",):
                    continue
                items.append({"kind": kind.rstrip("!"), "name": name, "line": i + 1, "depth": len(m.group(1)) // 4, "sig": l.strip()[:200], "doc": preceding_doc(lines, i)})
        elif ext in {".ts", ".js", ".svelte"}:
            m = TS_ITEM.match(l)
            if m and len(m.group(1)) <= 4:
                kind = m.group(2)
                if kind in ("const", "let") and "=>" not in l and "function" not in l and not l.lstrip().startswith("export"):
                    continue
                items.append({"kind": kind, "name": m.group(3), "line": i + 1, "depth": len(m.group(1)) // 2, "sig": l.strip()[:200], "doc": preceding_doc(lines, i)})
        elif ext == ".py":
            m = PY_ITEM.match(l)
            if m and len(m.group(1)) <= 4:
                items.append({"kind": m.group(2), "name": m.group(3), "line": i + 1, "depth": len(m.group(1)) // 4, "sig": l.strip()[:200], "doc": ""})
        elif ext == ".sh":
            m = SH_ITEM.match(l)
            if m:
                items.append({"kind": "function", "name": m.group(2), "line": i + 1, "depth": 0, "sig": l.strip()[:200], "doc": ""})
        elif ext == ".md":
            m = MD_ITEM.match(l)
            if m:
                items.append({"kind": "h" + str(len(m.group(1))), "name": m.group(2).strip(), "line": i + 1, "depth": len(m.group(1)) - 1, "sig": l.strip()[:200], "doc": ""})
    return items


def rust_uses(lines: list[str]) -> list[str]:
    out = set()
    for l in lines:
        s = l.strip()
        if s.startswith("use ") or s.startswith("pub use "):
            m = re.match(r"(?:pub\s+)?use\s+((?:crate|super|self|flightplan_server|flightplan)(?:::[A-Za-z0-9_]+)+)", s)
            if m:
                out.add(m.group(1))
        for m in re.finditer(r"\b(crate|flightplan_server)::([a-z_][a-z0-9_]*(?:::[a-z_][a-z0-9_]*)?)", s):
            out.add(f"{m.group(1)}::{m.group(2)}")
    return sorted(out)


class Index:
    def __init__(self) -> None:
        self.files: dict[str, dict] = {}
        self.loaded = False
        self.refreshed = 0.0

    def load(self) -> None:
        if self.loaded:
            return
        self.loaded = True
        try:
            data = json.loads(CACHE.read_text())
            if data.get("version") == INDEX_VERSION:
                self.files = data["files"]
        except Exception:
            self.files = {}

    def save(self) -> None:
        CACHE.parent.mkdir(parents=True, exist_ok=True)
        tmp = CACHE.with_suffix(".tmp")
        tmp.write_text(json.dumps({"version": INDEX_VERSION, "files": self.files}))
        tmp.replace(CACHE)

    def refresh(self, force: bool = False) -> dict:
        """Re-parse files whose size/mtime changed; drop deleted ones. Cheap when nothing changed."""
        self.load()
        if not force and time.time() - self.refreshed < 1.0:
            return {"changed": 0}
        changed = 0
        present = set()
        for f in list_files():
            p = ROOT / f
            try:
                st = p.stat()
            except OSError:
                continue
            if st.st_size > MAX_FILE:
                continue
            present.add(f)
            old = self.files.get(f)
            if old and not force and old["size"] == st.st_size and old["mtime"] == st.st_mtime:
                continue
            try:
                text = p.read_text(errors="replace")
            except OSError:
                continue
            lines = text.splitlines()
            self.files[f] = {
                "size": st.st_size,
                "mtime": st.st_mtime,
                "lines": len(lines),
                "sha": hashlib.sha1(text.encode()).hexdigest()[:12],
                "doc": module_doc(f, lines),
                "items": outline_of(f, lines),
                "uses": rust_uses(lines) if f.endswith(".rs") else [],
            }
            changed += 1
        for f in list(self.files):
            if f not in present:
                del self.files[f]
                changed += 1
        if changed:
            self.save()
        self.refreshed = time.time()
        return {"changed": changed}


INDEX = Index()


# ----------------------------------------------------------------------------------------------
# tools


def resolve(path: str) -> str:
    """Accept a repo path, an absolute path, or a unique suffix such as 'ui/route.rs'."""
    p = path.strip().lstrip("./")
    if p.startswith(str(ROOT)):
        p = str(Path(p).relative_to(ROOT))
    if p in INDEX.files:
        return p
    hits = [f for f in INDEX.files if f.endswith("/" + p) or f.endswith(p)]
    if len(hits) == 1:
        return hits[0]
    if not hits:
        raise ValueError(f"no indexed file matches {path!r}")
    raise ValueError(f"{path!r} is ambiguous: " + ", ".join(hits[:10]))


def tool_overview(args: dict) -> str:
    parts = []
    if OVERVIEW.exists():
        parts.append(OVERVIEW.read_text())
    groups: dict[str, list[str]] = {}
    for f, info in sorted(INDEX.files.items()):
        if f.endswith(".md") and not f.startswith("docs/"):
            continue
        d = str(Path(f).parent)
        doc = info["doc"].split(". ")[0][:140] if info["doc"] else ""
        groups.setdefault(d, []).append(f"  {Path(f).name} ({info['lines']} lines){': ' + doc if doc else ''}")
    want = args.get("dir")
    lines = ["", "## File map (auto-generated, one line per file)"]
    for d, entries in groups.items():
        if want and not d.startswith(want.strip("/")):
            continue
        lines.append(f"{d}/")
        lines.extend(entries)
    parts.append("\n".join(lines))
    if NOTES.exists() and not want:
        parts.append("\n## Notes from earlier agents\n" + NOTES.read_text().split("\n", 1)[-1][-6000:])
    return "\n".join(parts)


def tool_outline(args: dict) -> str:
    f = resolve(args["path"])
    info = INDEX.files[f]
    out = [f"{f} · {info['lines']} lines"]
    if info["doc"]:
        out.append(info["doc"])
    for it in info["items"]:
        doc = f"  // {it['doc']}" if it.get("doc") and args.get("docs", True) else ""
        out.append(f"{it['line']:>5}  {'  ' * it['depth']}{it['sig']}{doc}")
    if info.get("uses") and args.get("uses", False):
        out.append("uses: " + ", ".join(info["uses"]))
    return "\n".join(out)


def tool_find(args: dict) -> str:
    q = args["query"]
    kind = args.get("kind")
    try:
        rx = re.compile(q, re.I)
    except re.error:
        rx = re.compile(re.escape(q), re.I)
    exact, partial = [], []
    for f, info in INDEX.files.items():
        if args.get("path") and not fnmatch.fnmatch(f, args["path"]) and args["path"] not in f:
            continue
        for it in info["items"]:
            if kind and it["kind"] != kind:
                continue
            if it["name"].lower() == q.lower():
                exact.append((f, it))
            elif rx.search(it["name"]):
                partial.append((f, it))
    hits = exact + partial
    limit = int(args.get("limit", 40))
    out = [f"{f}:{it['line']}  {it['sig']}" + (f"  // {it['doc']}" if it.get("doc") else "") for f, it in hits[:limit]]
    if len(hits) > limit:
        out.append(f"... {len(hits) - limit} more (narrow with kind= or path=)")
    return "\n".join(out) or "no symbols match"


def block_end(lines: list[str], start: int, ext: str) -> int:
    """Index (exclusive) of the end of the item starting at line `start` (0-based)."""
    if ext == ".py":
        indent = len(lines[start]) - len(lines[start].lstrip())
        j = start + 1
        while j < len(lines) and (not lines[j].strip() or len(lines[j]) - len(lines[j].lstrip()) > indent):
            j += 1
        return j
    depth, seen = 0, False
    for j in range(start, len(lines)):
        s = re.sub(r'"(?:\\.|[^"\\])*"', '""', lines[j])
        s = re.sub(r"//.*", "", s)
        for ch in s:
            if ch in "{(":
                depth += 1
                seen = seen or ch == "{"
            elif ch in "})":
                depth -= 1
        if (seen and depth <= 0) or (not seen and s.rstrip().endswith(";") and depth <= 0):
            return j + 1
    return min(len(lines), start + 80)


def tool_snippet(args: dict) -> str:
    f = resolve(args["path"])
    lines = (ROOT / f).read_text(errors="replace").splitlines()
    if "symbol" in args and args["symbol"]:
        name = args["symbol"]
        items = [it for it in INDEX.files[f]["items"] if it["name"] == name or it["name"].endswith(name)]
        if not items:
            raise ValueError(f"{name!r} not found in {f}; see outline")
        out = []
        for it in items[: int(args.get("limit", 3))]:
            s = it["line"] - 1
            # include doc comments / attributes directly above
            while s > 0 and lines[s - 1].strip().startswith(("///", "#[", "//", "@")):
                s -= 1
            e = block_end(lines, it["line"] - 1, Path(f).suffix)
            out.append(f"{f}:{s + 1}-{e}\n" + "\n".join(f"{n + 1:>5}  {lines[n]}" for n in range(s, e)))
        return "\n\n".join(out)
    start = max(1, int(args.get("start", 1)))
    end = min(len(lines), int(args.get("end", start + 60)))
    return f"{f}:{start}-{end}\n" + "\n".join(f"{n:>5}  {lines[n - 1]}" for n in range(start, end + 1))


def tool_search(args: dict) -> str:
    pat = args["pattern"]
    try:
        rx = re.compile(pat, 0 if args.get("case", False) else re.I)
    except re.error as e:
        raise ValueError(f"bad regex: {e}")
    glob = args.get("path")
    ctx = int(args.get("context", 0))
    limit = int(args.get("limit", 60))
    out, n = [], 0
    for f in sorted(INDEX.files):
        if glob and not (fnmatch.fnmatch(f, glob) or glob in f):
            continue
        try:
            lines = (ROOT / f).read_text(errors="replace").splitlines()
        except OSError:
            continue
        for i, l in enumerate(lines):
            if rx.search(l):
                n += 1
                if n > limit:
                    continue
                if ctx:
                    lo, hi = max(0, i - ctx), min(len(lines), i + ctx + 1)
                    out.append(f"{f}:{i + 1}\n" + "\n".join(f"{k + 1:>5}{'>' if k == i else ' '} {lines[k]}" for k in range(lo, hi)))
                else:
                    out.append(f"{f}:{i + 1}: {l.strip()[:220]}")
    if n > limit:
        out.append(f"... {n - limit} more matches (narrow with path=)")
    return "\n".join(out) or "no matches"


def tool_refs(args: dict) -> str:
    """Where a symbol is used: word-boundary matches outside its own definition line."""
    name = args["symbol"]
    return tool_search({"pattern": rf"\b{re.escape(name)}\b", "path": args.get("path"), "limit": args.get("limit", 60), "case": True})


def tool_deps(args: dict) -> str:
    f = resolve(args["path"])
    info = INDEX.files[f]
    out = [f"{f} uses:"] + [f"  {u}" for u in info.get("uses", [])]
    # reverse: which files mention this module path
    if f.endswith(".rs"):
        p = Path(f)
        crate = "flightplan_server" if f.startswith("server/") else "crate"
        mod = ".".join(p.with_suffix("").parts[2:]).replace(".", "::")
        if mod.endswith("::mod") or mod in ("lib", "main"):
            mod = mod.rsplit("::", 1)[0]
        needle = f"{crate}::{mod}" if crate == "crate" else f"flightplan_server::{mod}"
        users = sorted(g for g, i in INDEX.files.items() if g != f and any(u == needle or u.startswith(needle + "::") for u in i.get("uses", [])))
        short = mod.split("::")[-1]
        sib = sorted(g for g, i in INDEX.files.items() if g != f and g.startswith(str(p.parent)) and any(u.startswith(f"super::{short}") for u in i.get("uses", [])))
        out.append(f"used by ({needle}):")
        out += [f"  {u}" for u in users + sib] or ["  (none found)"]
    return "\n".join(out)


def tool_notes(args: dict) -> str:
    if not NOTES.exists():
        return "No notes yet. Add one with add_note."
    text = NOTES.read_text()
    topic = args.get("topic")
    if not topic:
        return text
    blocks = re.split(r"(?m)^(?=## )", text)
    hits = [b for b in blocks if topic.lower() in b.lower()]
    return "\n".join(hits) or f"no notes mention {topic!r}"


def tool_add_note(args: dict) -> str:
    topic = args["topic"].strip()
    text = args["text"].strip()
    DOCS.mkdir(parents=True, exist_ok=True)
    if not NOTES.exists():
        NOTES.write_text("# Notes for agents\n\nGotchas, decisions and conventions learned while working on this repo. Add with the\nflightplan-context `add_note` tool; keep each note short and specific.\n")
    body = NOTES.read_text()
    stamp = time.strftime("%Y-%m-%d")
    head = f"## {topic}"
    entry = f"- {text} ({stamp})\n"
    if head in body:
        i = body.index(head) + len(head)
        nxt = body.find("\n## ", i)
        nxt = len(body) if nxt < 0 else nxt + 1
        body = body[:nxt].rstrip("\n") + "\n" + entry + ("\n" + body[nxt:] if nxt < len(body) else "")
    else:
        body = body.rstrip("\n") + f"\n\n{head}\n{entry}"
    NOTES.write_text(body)
    return f"Noted under '{topic}'."


def tool_recent(args: dict) -> str:
    n = int(args.get("count", 10))
    log = subprocess.run(["git", "log", f"-{n}", "--stat", "--format=%n%h %ad %s", "--date=short"], cwd=ROOT, capture_output=True, text=True).stdout
    status = subprocess.run(["git", "status", "--short"], cwd=ROOT, capture_output=True, text=True).stdout
    return ("Uncommitted:\n" + status if status.strip() else "Working tree clean.\n") + log


def tool_reindex(args: dict) -> str:
    r = INDEX.refresh(force=True)
    return f"Reindexed {len(INDEX.files)} files ({r['changed']} parsed)."


TOOLS = {
    "overview": (tool_overview, "Start here. Architecture overview (docs/context/OVERVIEW.md), a one-line-per-file map of the repo, and notes from earlier agents. Optional `dir` narrows the file map (e.g. 'native/src/ui').",
                 {"dir": {"type": "string", "description": "only list files under this directory"}}, []),
    "outline": (tool_outline, "Outline of one file without reading it: module doc, then every fn/struct/enum/impl/const (or TS/Python/shell/markdown items) with line number, signature and doc comment. Path may be a unique suffix like 'ui/route.rs'.",
                {"path": {"type": "string"}, "docs": {"type": "boolean", "description": "include doc comments (default true)"}, "uses": {"type": "boolean", "description": "also list crate paths the file uses"}}, ["path"]),
    "find": (tool_find, "Find symbol definitions by name (exact match first, then regex/substring). Returns path:line and signature.",
             {"query": {"type": "string"}, "kind": {"type": "string", "description": "fn, struct, enum, impl, const, trait, mod, function, class, def ..."}, "path": {"type": "string", "description": "glob or substring to limit files"}, "limit": {"type": "integer"}}, ["query"]),
    "snippet": (tool_snippet, "Source of one symbol (whole fn/struct/impl block, with its doc comment) or a line range. Use instead of reading a whole file.",
                {"path": {"type": "string"}, "symbol": {"type": "string"}, "start": {"type": "integer"}, "end": {"type": "integer"}, "limit": {"type": "integer"}}, ["path"]),
    "search": (tool_search, "Regex search across indexed source files (case-insensitive by default). Returns path:line: text, optionally with context lines.",
               {"pattern": {"type": "string"}, "path": {"type": "string", "description": "glob or substring, e.g. 'native/src/ui/*' or 'server/'"}, "context": {"type": "integer"}, "case": {"type": "boolean"}, "limit": {"type": "integer"}}, ["pattern"]),
    "refs": (tool_refs, "Usages of a symbol name (word-boundary, case-sensitive) across the repo.",
             {"symbol": {"type": "string"}, "path": {"type": "string"}, "limit": {"type": "integer"}}, ["symbol"]),
    "deps": (tool_deps, "For a Rust file: the crate paths it uses and the files that use it (impact of a change).",
             {"path": {"type": "string"}}, ["path"]),
    "notes": (tool_notes, "Read the shared notes (gotchas, decisions, conventions) left by earlier agents; optional topic filter.",
              {"topic": {"type": "string"}}, []),
    "add_note": (tool_add_note, "Save a short, durable note for future agents (a gotcha, decision or convention you had to discover). Stored in docs/context/NOTES.md under a topic heading.",
                 {"topic": {"type": "string", "description": "e.g. 'Build', 'egui', 'Weather data', 'Testing'"}, "text": {"type": "string"}}, ["topic", "text"]),
    "recent": (tool_recent, "Uncommitted changes plus the last N commits with the files they touched.",
               {"count": {"type": "integer"}}, []),
    "reindex": (tool_reindex, "Force a full re-parse of the index (normally automatic and incremental).", {}, []),
}


# ----------------------------------------------------------------------------------------------
# MCP over stdio


def send(msg: dict) -> None:
    sys.stdout.write(json.dumps(msg) + "\n")
    sys.stdout.flush()


def handle(req: dict) -> dict | None:
    method = req.get("method")
    rid = req.get("id")
    if rid is None:
        return None  # notifications (initialized, cancelled, ...)
    if method == "initialize":
        want = req.get("params", {}).get("protocolVersion") or PROTOCOL
        return {"jsonrpc": "2.0", "id": rid, "result": {
            "protocolVersion": want,
            "capabilities": {"tools": {"listChanged": False}},
            "serverInfo": {"name": "flightplan-context", "version": "1.0.0"},
            "instructions": "Cached map of the Flightplan repo. Call `overview` first, then `outline`/`find`/`snippet` instead of reading whole files. Record hard-won gotchas with `add_note`.",
        }}
    if method == "ping":
        return {"jsonrpc": "2.0", "id": rid, "result": {}}
    if method == "tools/list":
        tools = [{"name": n, "description": d, "inputSchema": {"type": "object", "properties": props, "required": req_}} for n, (_, d, props, req_) in TOOLS.items()]
        return {"jsonrpc": "2.0", "id": rid, "result": {"tools": tools}}
    if method == "tools/call":
        p = req.get("params", {})
        name, args = p.get("name"), p.get("arguments") or {}
        if name not in TOOLS:
            return {"jsonrpc": "2.0", "id": rid, "error": {"code": -32602, "message": f"unknown tool {name}"}}
        try:
            INDEX.refresh()
            text = TOOLS[name][0](args)
            return {"jsonrpc": "2.0", "id": rid, "result": {"content": [{"type": "text", "text": text}], "isError": False}}
        except Exception as e:  # tool errors go back to the model, not the transport
            return {"jsonrpc": "2.0", "id": rid, "result": {"content": [{"type": "text", "text": f"error: {e}"}], "isError": True}}
    if method in ("resources/list", "prompts/list"):
        key = "resources" if method.startswith("resources") else "prompts"
        return {"jsonrpc": "2.0", "id": rid, "result": {key: []}}
    return {"jsonrpc": "2.0", "id": rid, "error": {"code": -32601, "message": f"method not found: {method}"}}


def serve() -> None:
    for line in sys.stdin:
        line = line.strip()
        if not line:
            continue
        try:
            req = json.loads(line)
        except json.JSONDecodeError:
            send({"jsonrpc": "2.0", "id": None, "error": {"code": -32700, "message": "parse error"}})
            continue
        batch = req if isinstance(req, list) else [req]
        for r in batch:
            resp = handle(r)
            if resp is not None:
                send(resp)


def selftest() -> None:
    t0 = time.time()
    r = INDEX.refresh(force=True)
    print(f"indexed {len(INDEX.files)} files ({r['changed']} parsed) in {time.time() - t0:.2f}s")
    for name, args in [("overview", {"dir": "native/src/nav"}), ("outline", {"path": "nav/hazards.rs"}), ("find", {"query": "compute_wb"}),
                       ("snippet", {"path": "model/route.rs", "symbol": "departure_time"}), ("search", {"pattern": "mesonet", "limit": 5}),
                       ("refs", {"symbol": "fit_route", "limit": 5}), ("deps", {"path": "native/src/nav/hazards.rs"})]:
        out = TOOLS[name][0](args)
        print(f"--- {name} {args} ({len(out)} chars)\n" + "\n".join(out.splitlines()[:12]))


if __name__ == "__main__":
    if "--selftest" in sys.argv:
        selftest()
    else:
        serve()
