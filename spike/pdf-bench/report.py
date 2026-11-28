"""Summarise /out/results.json as markdown (/out/report.md)."""
import json, statistics as st
from collections import defaultdict
R = json.load(open("/out/results.json")); T = ["poppler", "pdfium", "mupdf"]; o = []
def med(x): x = [v for v in x if v is not None]; return round(st.median(x), 3) if x else ""
def mean(x): x = [v for v in x if v is not None]; return round(st.mean(x), 3) if x else ""
def q(x, p): x = sorted(v for v in x if v is not None); return round(x[int(p * (len(x) - 1))], 3) if x else ""
def by(rows, key):
    d = defaultdict(list)
    for r in rows: d[r[key]].append(r)
    return d
o.append(f"{len({r['file'] for r in R})} files, {len(R)//3} per tool. Tiers: " + ", ".join(f"{k} {len(v)//3}" for k, v in by(R, 'tier').items()))
o.append("\n## Success and speed, all files\n\n| tool | exit 0 | failed | killed/timeout | median wall ms | median ms/page | max RSS MB | total s |\n|---|---|---|---|---|---|---|---|")
for t in T:
    rs = [r for r in R if r["tool"] == t]
    o.append(f"| {t} | {sum(r['exits']==[0] for r in rs)} | {sum(r['exits']==[1] for r in rs)} | {sum(any(c<0 for c in r['exits']) for r in rs)} | {med([r['wall_ms'] for r in rs])} | {med([r['wall_ms']/r['pages'] for r in rs if r['pages']])} | {max(r['rss_mb'] for r in rs)} | {round(sum(r['wall_ms'] for r in rs)/1000,1)} |")
o.append("\n(PDFium wall time includes about 35 ms of Python and pypdfium2 start-up; `in-proc` below is the extraction alone.)")
o.append("\n## PDFium in-process vs wall, real tier\n\n| tool | median ms/page wall | median ms/page in-proc |\n|---|---|---|")
for t in T:
    rs = [r for r in R if r["tool"] == t and r["tier"] == "public" and r["pages"]]
    o.append(f"| {t} | {med([r['wall_ms']/r['pages'] for r in rs])} | {med([r['inproc_ms']/r['pages'] for r in rs if r['inproc_ms']]) if t=='pdfium' else ''} |")
g = [r for r in R if r["tier"] == "generated"]
o.append("\n## Known text (generated tier): similarity to truth\n\nsim = normalised edit similarity (reading order matters), f1 = word overlap (order free).\n\n| tool | mean sim | p10 sim | worst sim | mean f1 | files sim < 0.99 |\n|---|---|---|---|---|---|")
for t in T:
    rs = [r for r in g if r["tool"] == t and "sim" in r]
    o.append(f"| {t} | {mean([r['sim'] for r in rs])} | {q([r['sim'] for r in rs], .1)} | {min(r['sim'] for r in rs)} | {mean([r['f1'] for r in rs])} | {sum(r['sim']<0.99 for r in rs)} |")
o.append("\n### By layout (mean sim; only strictly ordered docs)\n\n| layout | n | poppler | pdfium | mupdf |\n|---|---|---|---|---|")
lay = defaultdict(lambda: defaultdict(list))
for r in g:
    if "sim" in r and r.get("ordered", True): lay[r.get("layout", "?")][r["tool"]].append(r["sim"])
for k, v in sorted(lay.items()): o.append(f"| {k} | {len(v['poppler'])} | " + " | ".join(str(mean(v[t])) for t in T) + " |")
o.append("\n### Every generated file where tools differ by more than 0.01 (sim; f1 in brackets)\n\n| file | layout | poppler | pdfium | mupdf |\n|---|---|---|---|---|")
byf = defaultdict(dict)
for r in g: byf[r["file"]][r["tool"]] = r
for f, d in sorted(byf.items()):
    s = [d[t].get("sim") for t in T]
    if None in s or max(s) - min(s) > 0.01 or min(s) < 0.97:
        o.append(f"| {f} | {d['poppler'].get('layout','')}{'' if d['poppler'].get('ordered',True) else ' (order ambiguous)'} | " + " | ".join(f"{d[t].get('sim','FAIL')} ({d[t].get('f1','')})" for t in T) + " |")
o.append("\n## Text integrity signals, all tiers except mutated\n\njunk = share of U+FFFD, private-use and control characters; lig = ligature code points left in the text (they break search for `office`); dict = share of English words found in a word list (real English docs); empty = pages with no text.\n\n| tool | mean junk | total lig chars | bidi control chars | stray control chars replaced | median dict | empty pages |\n|---|---|---|---|---|---|---|")
for t in T:
    rs = [r for r in R if r["tool"] == t and r["tier"] != "mutated" and "junk" in r]
    o.append(f"| {t} | {mean([r['junk'] for r in rs])} | {sum(r['lig'] for r in rs)} | {sum(r['bidi'] for r in rs)} | {sum(r.get('stray_ctl',0) for r in rs)} | {med([r['dict_rate'] for r in rs])} | {sum(r['empty_pages'] for r in rs)} |")
p = [r for r in R if r["tier"] == "public"]
o.append("\n## Real PDFs: agreement (word F1 against the other two tools), integrity and speed\n\n| file | pages | poppler agree / dict / lig | pdfium | mupdf | ms/page P / F / M |\n|---|---|---|---|---|---|")
byf = defaultdict(dict)
for r in p: byf[r["file"]][r["tool"]] = r
for f, d in sorted(byf.items()):
    cell = lambda r: f"{r.get('agree','FAIL')} / {r.get('dict_rate','')} / {r.get('lig','')}" if r["exits"] == [0] else f"exit {r['exits']}"
    pg = max(d[t]["pages"] for t in T) or 1
    o.append(f"| {f[:44]} | {pg} | " + " | ".join(cell(d[t]) for t in T) + " | " + " / ".join(str(round(d[t]['wall_ms']/pg, 1)) for t in T) + " |")
o.append("\n## Damaged files (mutated tier): what each tool does\n\nchars = text recovered (median over the 30 files vs the best tool for the same file).\n\n| tool | exit 0 | exit 1 | killed | recovered vs best tool (median) | flaky |\n|---|---|---|---|---|---|")
m = [r for r in R if r["tier"] == "mutated"]; best = defaultdict(int)
for r in m: best[r["file"]] = max(best[r["file"]], r["chars"])
for t in T:
    rs = [r for r in m if r["tool"] == t]
    o.append(f"| {t} | {sum(r['exits']==[0] for r in rs)} | {sum(r['exits']==[1] for r in rs)} | {sum(any(c<0 for c in r['exits']) for r in rs)} | {med([r['chars']/best[r['file']] for r in rs if best[r['file']]])} | {sum(r['flaky'] for r in rs)} |")
o.append("\n### Damaged files where tools disagree\n\n| file | poppler | pdfium | mupdf |\n|---|---|---|---|")
byf = defaultdict(dict)
for r in m: byf[r["file"]][r["tool"]] = r
for f, d in sorted(byf.items()):
    cs = [(d[t]["exits"], d[t]["chars"]) for t in T]
    if len({(e[0] == 0, c > 0) for e, c in cs}) > 1: o.append(f"| {f[:50]} | " + " | ".join(f"exit {e} chars {c}" for e, c in cs) + " |")
open("/out/report.md", "w").write("\n".join(o) + "\n")
print("\n".join(o)[:200])
