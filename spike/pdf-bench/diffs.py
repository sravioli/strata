import difflib, subprocess, sys
sys.argv = sys.argv[:1] + sys.argv[1:]
from pathlib import Path
QU = str.maketrans({"’": "'", "‘": "'", "“": "'", "”": "'", '"': "'", "–": "-", "—": "-", "­": ""})
norm = lambda s: " ".join(s.replace("\f", " ").translate(QU).split())
cmds = {"poppler": ["pdftotext", "-q", "-enc", "UTF-8", "{}", "-"], "pdfium": ["python3", "/bench/pdfium_text.py", "{}"], "mupdf": ["mutool", "draw", "-q", "-F", "text", "-o", "-", "{}"]}
for name in sys.argv[1:]:
    pdf = Path("/corpus/generated") / f"{name}.pdf"; truth = norm((pdf.with_suffix(".txt")).read_text())
    print("=====", name, "truth chars", len(truth))
    for t, c in cmds.items():
        out = norm(subprocess.run([a.format(pdf) for a in c], capture_output=True, text=True).stdout)
        sm = difflib.SequenceMatcher(None, truth, out, autojunk=False) if len(truth) < 6000 else None
        w = difflib.SequenceMatcher(None, truth.split(), out.split(), autojunk=False)
        ops = [o for o in w.get_opcodes() if o[0] != "equal"][:4]
        print(f"-- {t} chars {len(out)}; first word diffs:")
        for tag, i1, i2, j1, j2 in ops: print("   ", tag, "TRUTH:", " ".join(truth.split()[i1:i2])[:90], "| GOT:", " ".join(out.split()[j1:j2])[:90])
