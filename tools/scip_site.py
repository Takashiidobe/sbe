#!/usr/bin/env python3
"""Build a static, clickable browser view of the SCIP index.

    tools/scip_site.py            # writes .scip/site/ from .scip/index.scip
    xdg-open .scip/site/index.html

Click any identifier to see where it is defined and every place it is referenced, with the
enclosing function of each use. Rebuild the index first with tools/index.sh if sources changed.
"""

import html
import json
import re
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


IMPL_OWNER = re.compile(r"impl#\[`?([^\]`<]+)")


def label(symbol, display):
    """`Function::params` for methods, so same-named functions can be told apart."""
    if not symbol.endswith("()."):
        return display
    descriptor = symbol.rsplit("/", 1)[-1]
    owner = IMPL_OWNER.match(descriptor)
    if owner:
        return f"{owner.group(1)}::{display}"
    if "#" in descriptor:
        return f"{descriptor.split('#', 1)[0]}::{display}"
    return display


def collect(index):
    names = {}
    for doc in index["documents"]:
        for info in doc.get("symbols", []):
            symbol = info["symbol"]
            names[symbol] = label(symbol, info.get("display_name") or symbol)

    defs = {}
    refs = defaultdict(list)
    pending = []
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
            if inside and symbol.endswith("()."):
                pending.append((inside[0][1], symbol, path, line + 1))

    seen, calls = set(), []
    for caller, callee, path, line in pending:
        if caller in defs and callee in defs and (caller, callee) not in seen:
            seen.add((caller, callee))
            calls.append((caller, callee, path, line))
    return names, defs, refs, calls


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
let current = null, depth = 2;
const callers = new Map(), callees = new Map();
for (const [a, b] of CALLS) {
  if (!callees.has(a)) callees.set(a, []);
  callees.get(a).push(b);
  if (!callers.has(b)) callers.set(b, []);
  callers.get(b).push(a);
}
function levels(start, adj, max) {
  const seen = new Set([start]);
  let frontier = [start];
  const out = [];
  for (let d = 1; d <= max && frontier.length; d++) {
    const next = [];
    for (const n of frontier)
      for (const m of adj.get(n) || []) if (!seen.has(m)) { seen.add(m); next.push(m); }
    if (next.length) out.push(next);
    frontier = next;
  }
  return out;
}
function callList(title, sym, adj) {
  const lv = levels(sym, adj, depth);
  let h = `<h2>${title}</h2>`;
  if (!lv.length) return h + '<p class="caller">none</p>';
  lv.forEach((nodes, i) => {
    h += `<div class="caller">depth ${i + 1}</div><ul>`;
    for (const n of nodes.sort((a, b) => NAMES[a].localeCompare(NAMES[b]))) {
      const d = DEFS[n];
      h += `<li><a href="${link(d[0], d[1])}">${esc(NAMES[n])}</a> <span class="caller">${esc(d[0])}</span></li>`;
    }
    h += '</ul>';
  });
  return h;
}
function callSection(sym) {
  const opts = [1, 2, 3, 4, 5].map(n => `<option${n === depth ? ' selected' : ''}>${n}</option>`).join('');
  return `<h2>Call graph <select id="depth">${opts}</select> ` +
    `<a href="graph.html?s=${encodeURIComponent(sym)}&d=${depth}">open graph</a></h2>` +
    callList('Callers', sym, callers) + callList('Callees', sym, callees);
}
document.addEventListener('change', e => {
  if (e.target.id === 'depth') { depth = +e.target.value; show(current); }
});
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
  if (sym.endsWith('().')) h += callSection(sym);
  current = sym;
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


GRAPH_HTML = """<!doctype html><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1">
<title>call graph</title><link rel="stylesheet" href="style.css">
<header><a href="index.html">index</a><b id="title"></b>
<label>depth <select id="d"><option>1</option><option>2</option><option>3</option><option>4</option></select></label></header>
<div id="msg" style="padding:16px"></div>
<div style="overflow:auto"><svg id="g" xmlns="http://www.w3.org/2000/svg"></svg></div>
<script src="data.js"></script><script src="graph.js"></script>"""

GRAPH_JS = """
const params = new URLSearchParams(location.search);
const root = params.get('s');
const maxDepth = +(params.get('d') || 2);
const NODE_W = 300, NODE_H = 24, COL_GAP = 90, ROW_GAP = 8;
const NS = 'http://www.w3.org/2000/svg';
const adjOut = new Map(), adjIn = new Map();
for (const [a, b] of CALLS) {
  if (!adjOut.has(a)) adjOut.set(a, []);
  adjOut.get(a).push(b);
  if (!adjIn.has(b)) adjIn.set(b, []);
  adjIn.get(b).push(a);
}
function column(adj, sign) {
  const col = new Map(), seen = new Set([root]);
  let frontier = [root];
  for (let d = 1; d <= maxDepth && frontier.length; d++) {
    const next = [];
    for (const n of frontier)
      for (const m of adj.get(n) || []) if (!seen.has(m)) { seen.add(m); col.set(m, sign * d); next.push(m); }
    frontier = next;
  }
  return col;
}
document.getElementById('d').value = String(maxDepth);
document.getElementById('d').addEventListener('change', e => {
  params.set('d', e.target.value);
  location.search = params.toString();
});
if (!root || !DEFS[root]) {
  document.getElementById('msg').textContent = 'Pick a function from a code page (side panel, "open graph").';
} else {
  document.getElementById('title').textContent = NAMES[root];
  const cols = new Map([[root, 0], ...column(adjIn, -1), ...column(adjOut, 1)]);
  const byCol = new Map();
  for (const [n, c] of cols) (byCol.get(c) || byCol.set(c, []).get(c)).push(n);
  const keys = [...byCol.keys()].sort((a, b) => a - b);
  const minCol = keys[0];
  const rows = Math.max(...[...byCol.values()].map(v => v.length));
  const height = rows * (NODE_H + ROW_GAP) + 40;
  const width = keys.length * (NODE_W + COL_GAP) + 40;
  const pos = new Map();
  for (const c of keys) {
    const nodes = byCol.get(c).sort((a, b) => NAMES[a].localeCompare(NAMES[b]));
    const top = (height - nodes.length * (NODE_H + ROW_GAP)) / 2;
    nodes.forEach((n, i) => pos.set(n, { x: 20 + (c - minCol) * (NODE_W + COL_GAP), y: top + i * (NODE_H + ROW_GAP) }));
  }
  const svg = document.getElementById('g');
  svg.setAttribute('width', width);
  svg.setAttribute('height', height);
  const el = (tag, attrs, parent = svg) => {
    const e = document.createElementNS(NS, tag);
    for (const k in attrs) e.setAttribute(k, attrs[k]);
    parent.appendChild(e);
    return e;
  };
  const marker = el('marker', { id: 'arrow', viewBox: '0 0 10 10', refX: 9, refY: 5, markerWidth: 7, markerHeight: 7, orient: 'auto' });
  el('path', { d: 'M0 0L10 5L0 10z', fill: 'var(--mut)' }, marker);
  for (const [a, b] of CALLS) {
    if (!pos.has(a) || !pos.has(b) || a === b) continue;
    const p = pos.get(a), q = pos.get(b);
    if (q.x <= p.x) continue;
    const x1 = p.x + NODE_W, x2 = q.x;
    const y1 = p.y + NODE_H / 2, y2 = q.y + NODE_H / 2;
    const dx = Math.max(30, (x2 - x1) / 2);
    el('path', {
      d: `M${x1} ${y1}C${x1 + dx} ${y1} ${x2 - dx} ${y2} ${x2} ${y2}`,
      fill: 'none', stroke: 'var(--mut)', 'stroke-width': 1, 'marker-end': 'url(#arrow)',
    });
  }
  for (const [n, p] of pos) {
    const d = DEFS[n];
    const a = el('a', { href: `graph.html?s=${encodeURIComponent(n)}&d=${maxDepth}` });
    el('rect', { x: p.x, y: p.y, width: NODE_W, height: NODE_H, rx: 4,
      fill: n === root ? 'var(--hl)' : 'var(--pan)', stroke: 'var(--line)' }, a);
    const t = el('text', { x: p.x + 8, y: p.y + 16, 'font-size': 12, fill: 'var(--fg)',
      'font-family': 'ui-monospace, monospace' }, a);
    t.textContent = NAMES[n].length > 28 ? NAMES[n].slice(0, 27) + '…' : NAMES[n];
    const src = el('a', { href: `${pageName(d[0])}#L${d[1]}` });
    const s = el('text', { x: p.x + NODE_W - 8, y: p.y + 16, 'font-size': 11, 'text-anchor': 'end', fill: 'var(--acc)' }, src);
    s.textContent = d[0].split('/').pop().replace('.rs', '') + ':' + d[1];
    const tip = el('title', {}, a);
    tip.textContent = n;
  }
}
"""


def write_site(index):
    names, defs, refs, calls = collect(index)
    OUT.mkdir(parents=True, exist_ok=True)

    data = {
        "NAMES": {s: names.get(s, s) for s in set(defs) | set(refs)},
        "DEFS": defs,
        "REFS": refs,
        "CALLS": calls,
    }
    (OUT / "data.js").write_text(
        "".join(f"const {key} = {json.dumps(value)};\n" for key, value in data.items())
        + "const pageName = p => p.replaceAll('/', '__') + '.html';\n"
    )
    (OUT / "site.js").write_text(SCRIPT)
    (OUT / "graph.html").write_text(GRAPH_HTML)
    (OUT / "graph.js").write_text(GRAPH_JS)
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
