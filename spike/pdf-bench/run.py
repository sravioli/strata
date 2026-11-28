"""Run each extractor over each PDF REPS times. One row per (file, tool): time, peak memory, exit codes,
text quality against known text (generated tier) or against the other tools (real tier)."""
import json, os, re, resource, statistics as st, subprocess, sys, tempfile, threading, time
from collections import Counter
from pathlib import Path
from rapidfuzz.distance import Levenshtein

REPS = int(os.environ.get("REPS", "3")); TIMEOUT = 60; AS_LIMIT = 1 << 30; CAP = 150_000   # same limits for all
EXTRACTORS = {
    "poppler": ["pdftotext", "-q", "-enc", "UTF-8", "{pdf}", "-"],
    "pdfium": ["python3", "/bench/pdfium_text.py", "{pdf}"],
    "mupdf": ["mutool", "draw", "-q", "-F", "text", "-o", "-", "{pdf}"],
}
WORDS = {w.strip().lower() for w in open("/usr/share/dict/american-english", errors="ignore")}
QUOTES = str.maketrans({"’": "'", "‘": "'", "“": "'", "”": "'", '"': "'", "–": "-", "—": "-", "­": ""}) | {c: None for c in (*range(0x200E, 0x2010), *range(0x202A, 0x202F), *range(0x2066, 0x206A))}   # bidi marks: logical text only
def limits(): resource.setrlimit(resource.RLIMIT_AS, (AS_LIMIT, AS_LIMIT))
def norm(s): return " ".join(s.replace("\f", " ").translate(QUOTES).split())

def once(cmd):
    with tempfile.TemporaryFile() as o, tempfile.TemporaryFile() as e:
        t0 = time.perf_counter(); p = subprocess.Popen(cmd, stdout=o, stderr=e, preexec_fn=limits)
        timer = threading.Timer(TIMEOUT, p.kill); timer.start()
        _, status, ru = os.wait4(p.pid, 0); timer.cancel(); wall = time.perf_counter() - t0
        o.seek(0); e.seek(0); err = e.read().decode(errors="replace")
        inproc = next((float(l[2:]) for l in err.splitlines() if l.startswith("t=")), None)
        ctl = next((int(l[4:]) for l in err.splitlines() if l.startswith("ctl=")), 0)
        return dict(code=os.waitstatus_to_exitcode(status), wall=wall, rss=ru.ru_maxrss / 1024, inproc=inproc, ctl=ctl, text=o.read().decode(errors="replace"))

def unit(s, cjk): return list(s.replace(" ", "")) if cjk else s.split()
def f1(a, b, cjk=False):
    ca, cb = Counter(unit(a, cjk)), Counter(unit(b, cjk)); hit = sum((ca & cb).values())
    return 0.0 if not hit else 2 * hit / (sum(ca.values()) + sum(cb.values()))
def sim(a, b): return Levenshtein.normalized_similarity(a[:CAP], b[:CAP])
def signals(raw):
    n = max(1, len(raw)); toks = [t.lower() for t in re.findall(r"[A-Za-z]{3,}", raw)]
    pages = raw.split("\f")[:-1] or [raw]
    return dict(bidi=sum(0x200E <= ord(c) <= 0x200F or 0x202A <= ord(c) <= 0x202E or 0x2066 <= ord(c) <= 0x2069 for c in raw), junk=round(sum(c == "�" or "" <= c <= "" or (c < " " and c not in "\n\f\t\r") for c in raw) / n, 5),
                lig=sum("ﬀ" <= c <= "ﬆ" for c in raw), empty_pages=sum(len(p.strip()) < 5 for p in pages),
                dict_rate=round(sum(t in WORDS for t in toks) / len(toks), 4) if len(toks) > 200 else None)

def main(tiers):
    rows = []
    for tier, d in tiers:
        for pdf in sorted(Path(d).glob("*.pdf")):
            tf = pdf.with_suffix(".txt"); truth = norm(tf.read_text()) if tf.exists() else None
            meta = json.loads(pdf.with_suffix(".json").read_text()) if pdf.with_suffix(".json").exists() else {}
            cjk = meta.get("lang") in ("ja", "zh"); res = {}
            for name, tmpl in EXTRACTORS.items():
                cmd = [a.format(pdf=pdf) for a in tmpl]; runs = [once(cmd) for _ in range(REPS)]
                ok = [r for r in runs if r["code"] == 0]; raw = ok[0]["text"] if ok else ""
                res[name] = dict(runs=runs, ok=ok, raw=raw, text=norm(raw))
            for name, r in res.items():
                ok, text = r["ok"], r["text"]; others = [res[o]["text"] for o in res if o != name and res[o]["ok"]]
                row = dict(tier=tier, file=pdf.name, tool=name, bytes=pdf.stat().st_size, exits=sorted({x["code"] for x in r["runs"]}),
                           flaky=len({x["code"] for x in r["runs"]}) > 1, pages=max(1, len(r["raw"].split("\f")) - 1) if ok else 0, chars=len(text),
                           wall_ms=round(1000 * st.median(x["wall"] for x in r["runs"]), 1),
                           inproc_ms=round(1000 * st.median(x["inproc"] for x in ok), 1) if ok and ok[0]["inproc"] else None,
                           rss_mb=round(max(x["rss"] for x in r["runs"]), 1), stray_ctl=ok[0]["ctl"] if ok else 0, **meta)
                if ok: row.update(signals(r["raw"]))
                if ok and truth is not None: row.update(sim=round(sim(text, truth), 4), f1=round(f1(text, truth, cjk), 4))
                if ok and others and tier != "generated": row["agree"] = round(st.mean(f1(text, o, cjk) for o in others), 4)
                rows.append(row); print(json.dumps(row, ensure_ascii=False), flush=True)
    return rows

if __name__ == "__main__":
    json.dump(main([a.split("=") for a in sys.argv[1:]]), open("/out/results.json", "w"), indent=1, ensure_ascii=False)
