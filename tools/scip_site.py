#!/usr/bin/env python3
"""Build a static, clickable browser view of the SCIP index.

    tools/scip_site.py            # writes .scip/site/ from .scip/index.scip
    xdg-open .scip/site/index.html

Click any identifier to see where it is defined and every place it is referenced, with the
enclosing function of each use. Rebuild the index first with tools/index.sh if sources changed.
"""

import html
import json
import subprocess
import sys
from collections import defaultdict
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
INDEX = ROOT / ".scip" / "index.scip"
OUT = ROOT / ".scip" / "site"

DEFINITION = 1


def load_index():
    if not INDEX.exists():
        sys.exit(f"missing {INDEX}; run: rust-analyzer scip . --output .scip/index.scip")
    out = subprocess.run(
        ["scip", "print", "--json", str(INDEX)], check=True, capture_output=True, text=True
    )
    return json.loads(out.stdout)


def page_name(path):
    return path.replace("/", "__") + ".html"


def contains(outer, line, col):
    if len(outer) == 3:
        return line == outer[0] and outer[1] <= col < outer[2]
    return (outer[0], outer[1]) <= (line, col) < (outer[2], outer[3])


def span(rng):
    return (rng[2] - rng[0], 0) if len(rng) == 4 else (0, rng[2] - rng[1])


def collect(index):
    names = {}
    for doc in index["documents"]:
        for info in doc.get("symbols", []):
            names[info["symbol"]] = info.get("display_name") or info["symbol"]

    defs = {}
    refs = defaultdict(list)
    owners = {}
    for doc in index["documents"]:
        path = doc["relative_path"]
        items = [
            (o["enclosing_range"], o["symbol"])
            for o in doc.get("occurrences", [])
            if o.get("symbol_roles", 0) & DEFINITION
            and "enclosing_range" in o
            and o["symbol"].endswith("().")
        ]
        for occ in doc.get("occurrences", []):
            symbol = occ["symbol"]
            if symbol.startswith("local "):
                continue
            line, col = occ["range"][0], occ["range"][1]
            if occ.get("symbol_roles", 0) & DEFINITION:
                defs[symbol] = (path, line + 1, col + 1)
                continue
            inside = [e for e in items if contains(e[0], line, col)]
            inside.sort(key=lambda e: span(e[0]))
            caller = names.get(inside[0][1]) if inside else None
            refs[symbol].append((path, line + 1, col + 1, caller))
    return names, defs, refs


def render_file(doc, source, defs):
    lines = source.split("\n")
    by_line = defaultdict(list)
    for occ in doc.get("occurrences", []):
        rng = occ["range"]
        symbol = occ["symbol"]
        if len(rng) != 3 or symbol.startswith("local "):
            continue
        if symbol not in defs and not occ.get("symbol_roles", 0) & DEFINITION:
            continue
        by_line[rng[0]].append((rng[1], rng[2], symbol, bool(occ.get("symbol_roles", 0) & DEFINITION)))

    rows = []
    for number, text in enumerate(lines):
        pieces, cursor = [], 0
        for start, end, symbol, is_def in sorted(by_line.get(number, [])):
            if start < cursor:
                continue
            pieces.append(html.escape(text[cursor:start]))
            cls = "s d" if is_def else "s"
            pieces.append(
                f'<span class="{cls}" data-s="{html.escape(symbol, quote=True)}">'
                f"{html.escape(text[start:end])}</span>"
            )
            cursor = end
        pieces.append(html.escape(text[cursor:]))
        rows.append(
            f'<tr id="L{number + 1}"><td class="n">{number + 1}</td><td class="c">{"".join(pieces)}</td></tr>'
        )
    return "\n".join(rows)


STYLE = """
:root { --bg:#fff; --fg:#1f2328; --mut:#6e7781; --acc:#0969da; --line:#d0d7de; --hl:#fff8c5; --pan:#f6f8fa; }
@media (prefers-color-scheme: dark) {
  :root { --bg:#0d1117; --fg:#e6edf3; --mut:#8b949e; --acc:#58a6ff; --line:#30363d; --hl:#3d3300; --pan:#161b22; }
}
body { margin:0; background:var(--bg); color:var(--fg); font:14px/1.5 system-ui, sans-serif; }
a { color:var(--acc); text-decoration:none; } a:hover { text-decoration:underline; }
header { padding:8px 16px; border-bottom:1px solid var(--line); display:flex; gap:16px; align-items:baseline; }
main { display:flex; align-items:flex-start; }
.code { flex:1; min-width:0; overflow-x:auto; padding:8px 0; }
table { border-collapse:collapse; font:13px/1.45 ui-monospace, monospace; }
td.n { color:var(--mut); text-align:right; padding:0 12px; user-select:none; }
td.c { white-space:pre; padding-right:16px; }
tr.hl { background:var(--hl); }
.s { cursor:pointer; } .s:hover, .s.on { background:var(--hl); border-radius:2px; } .s.d { font-weight:600; }
aside { width:380px; flex:none; position:sticky; top:0; max-height:100vh; overflow:auto;
        padding:12px 16px; background:var(--pan); border-left:1px solid var(--line); }
aside h2 { font-size:13px; margin:12px 0 4px; } aside code { font-size:12px; word-break:break-all; }
aside ul { list-style:none; padding:0; margin:0; } aside li { padding:2px 0; font-size:13px; }
aside .caller { color:var(--mut); }
.files { padding:16px; columns:2; } .files li { list-style:none; padding:2px 0; }
input { width:100%; box-sizing:border-box; padding:6px 8px; font-size:14px; background:var(--bg);
        color:var(--fg); border:1px solid var(--line); border-radius:4px; }
@media (max-width:800px) { main { flex-direction:column; } aside { width:auto; position:static; max-height:none; } }
"""

SCRIPT = """
const panel = document.getElementById('panel');
const esc = s => s.replace(/[&<>"]/g, c => ({'&':'&amp;','<':'&lt;','>':'&gt;','"':'&quot;'}[c]));
const link = (p, l) => `${pageName(p)}#L${l}`;
function show(sym) {
  const d = DEFS[sym], r = REFS[sym] || [];
  let h = `<h2>${esc(NAMES[sym] || sym)}</h2><code>${esc(sym)}</code>`;
  h += '<h2>Definition</h2>' + (d ? `<ul><li><a href="${link(d[0], d[1])}">${esc(d[0])}:${d[1]}</a></li></ul>` : '<p>external</p>');
  const byFile = {};
  r.forEach(x => (byFile[x[0]] = byFile[x[0]] || []).push(x));
  h += `<h2>References (${r.length})</h2>`;
  for (const f of Object.keys(byFile).sort()) {
    h += `<div><b>${esc(f)}</b></div><ul>`;
    for (const [, l, , caller] of byFile[f])
      h += `<li><a href="${link(f, l)}">line ${l}</a>${caller ? ` <span class="caller">in ${esc(caller)}</span>` : ''}</li>`;
    h += '</ul>';
  }
  panel.innerHTML = h;
  document.querySelectorAll('.s.on').forEach(e => e.classList.remove('on'));
  document.querySelectorAll('.s').forEach(e => { if (e.dataset.s === sym) e.classList.add('on'); });
}
document.addEventListener('click', e => {
  const t = e.target.closest('.s');
  if (t) show(t.dataset.s);
});
if (location.hash.startsWith('#L')) {
  const row = document.getElementById(location.hash.slice(1));
  if (row) { row.classList.add('hl'); row.scrollIntoView({block:'center'}); }
}
"""


def write_site(index):
    names, defs, refs = collect(index)
    OUT.mkdir(parents=True, exist_ok=True)

    data = {
        "NAMES": {s: names.get(s, s) for s in set(defs) | set(refs)},
        "DEFS": defs,
        "REFS": refs,
    }
    (OUT / "data.js").write_text(
        "".join(f"const {key} = {json.dumps(value)};\n" for key, value in data.items())
        + "const pageName = p => p.replaceAll('/', '__') + '.html';\n"
    )
    (OUT / "site.js").write_text(SCRIPT)
    (OUT / "style.css").write_text(STYLE)

    paths = []
    for doc in sorted(index["documents"], key=lambda d: d["relative_path"]):
        path = doc["relative_path"]
        source_file = ROOT / path
        if not source_file.exists():
            continue
        paths.append(path)
        body = render_file(doc, source_file.read_text(), defs)
        (OUT / page_name(path)).write_text(
            f"""<!doctype html><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1">
<title>{html.escape(path)}</title><link rel="stylesheet" href="style.css">
<header><a href="index.html">index</a><b>{html.escape(path)}</b></header>
<main><div class="code"><table>{body}</table></div><aside id="panel"><p>Click an identifier.</p></aside></main>
<script src="data.js"></script><script src="site.js"></script>"""
        )

    items = "".join(f'<li><a href="{page_name(p)}">{html.escape(p)}</a></li>' for p in paths)
    (OUT / "index.html").write_text(
        f"""<!doctype html><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1">
<title>sbe code index</title><link rel="stylesheet" href="style.css">
<header><b>sbe code index</b></header>
<div style="padding:16px 16px 0"><input id="q" placeholder="Search symbols (e.g. lower_binary, InstData)" autofocus></div>
<ul id="hits" style="padding:0 16px"></ul>
<ul class="files">{items}</ul>
<script src="data.js"></script>
<script>
const hits = document.getElementById('hits');
document.getElementById('q').addEventListener('input', e => {{
  const q = e.target.value.toLowerCase();
  if (!q) {{ hits.innerHTML = ''; return; }}
  const found = Object.keys(DEFS).filter(s => NAMES[s].toLowerCase().includes(q)).slice(0, 40);
  hits.innerHTML = found.map(s => {{
    const [p, l] = DEFS[s];
    return `<li style="list-style:none"><a href="${{pageName(p)}}#L${{l}}">${{NAMES[s]}}</a> <span style="color:var(--mut)">${{p}}:${{l}}</span></li>`;
  }}).join('');
}});
</script>"""
    )
    print(f"wrote {len(paths)} pages to {OUT}")


if __name__ == "__main__":
    write_site(load_index())
