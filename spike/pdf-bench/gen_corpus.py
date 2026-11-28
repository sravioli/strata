"""Render real texts into PDFs with many layouts. <name>.pdf, <name>.txt (truth in reading order)
and <name>.json (tags: layout, lang, producer, ordered = whether the truth order is unambiguous)."""
import html, json, random, re, subprocess, sys, tempfile, unicodedata
from pathlib import Path
from weasyprint import HTML

SRC, OUT = Path(sys.argv[1]), Path(sys.argv[2]); OUT.mkdir(parents=True, exist_ok=True)
rnd = random.Random(11)

def paragraphs(name, skip=3):
    t = (SRC / f"{name}.txt").read_text(encoding="utf-8", errors="replace")
    t = re.sub(r"^=+\s*(.*?)\s*=+\s*$", r"\1", t, flags=re.M)          # wiki headings
    ps = [" ".join(p.split()) for p in re.split(r"\n\s*\n", t)]
    return [p for p in ps if len(p) > 80][skip:]
def take(ps, chars, start=0):
    start %= max(1, len(ps) // 2)          # a short source wraps to an earlier start
    out, n = [], 0
    for p in ps[start:]:
        out.append(p); n += len(p)
        if n >= chars: break
    return out
def ascii_(s):
    for a, b in {"’": "'", "‘": "'", "“": "'", "”": "'", "–": "-", "—": ", ", "…": "...", "\"": "'", "`": "'"}.items(): s = s.replace(a, b)
    return unicodedata.normalize("NFKD", s).encode("ascii", "ignore").decode()
def emit(name, pdf_writer, truth, **tags):
    assert sum(len(t) for t in truth) > 250, f"{name}: too little text ({sum(len(t) for t in truth)}), pick an earlier start"
    pdf_writer(OUT / f"{name}.pdf")
    (OUT / f"{name}.txt").write_text("\n".join(truth), encoding="utf-8")
    (OUT / f"{name}.json").write_text(json.dumps(dict(tags, producer=tags.get("producer", "weasyprint"))))
    print("made", name, flush=True)

CSS = '@page{size:A4;margin:2cm} body{font:11pt/1.4 "Noto Serif","DejaVu Serif",serif;margin:0} p{margin:0 0 .6em} h2{font-size:14pt;margin:.8em 0 .3em}'
def wp(css, body, extra_head=""):
    doc = f"<html><head><meta charset=utf-8><style>{CSS}{css}</style>{extra_head}</head><body>{body}</body></html>"
    return lambda path: HTML(string=doc).write_pdf(str(path))
def P(ps): return "".join(f"<p>{html.escape(p)}</p>" for p in ps)
EN = paragraphs("wiki-en2"); EN3 = paragraphs("wiki-en3"); EN4 = paragraphs("wiki-en4"); RFC = paragraphs("rfc-9110", 20)

# ---------- WeasyPrint layouts (English prose) ----------
emit("w01-prose-1col", wp("", P(take(EN, 40000))), take(EN, 40000), layout="1col")
emit("w02-dense-8pt", wp("body{font-size:8pt;line-height:1.15} @page{margin:1.2cm}", P(take(EN3, 40000))), take(EN3, 40000), layout="dense-small")
emit("w03-tiny-5pt", wp("body{font-size:5.5pt;line-height:1.1} @page{margin:1cm}", P(take(EN4, 30000))), take(EN4, 30000), layout="dense-small")
for cols, nm in ((2, "w04-2col"), (3, "w05-3col"), (4, "w06-4col-narrow")):
    ps = take(EN4, 30000, 5)
    emit(nm, wp(f"body{{column-count:{cols};column-gap:.6cm;font-size:{10 if cols < 4 else 8}pt}}", P(ps)), ps, layout="multicolumn")
ps = take(EN, 24000, 30)
emit("w07-justified-hyphens", wp("body{text-align:justify;hyphens:auto;column-count:2}", P(ps), ""), ps, layout="justified", ordered=True, lang_attr="en")
ps = take(EN3, 22000, 12)
emit("w08-running-header-footer", wp('@page{@top-center{content:"STRATA BENCH CONFIDENTIAL"} @bottom-right{content:"Page " counter(page) " of " counter(pages)}}', P(ps)), ps, layout="header-footer", ordered=False)
ps = take(EN, 20000, 60)
emit("w09-sidebar", wp(".side{float:right;width:5cm;border:1px solid #888;padding:6px;margin:0 0 8px 12px;font-size:9pt}", f'<div class=side>{html.escape(ps[0][:300])}</div>' + P(ps[1:])), [ps[0][:300]] + ps[1:], layout="sidebar", ordered=False)
ps = take(EN3, 14000, 40)
foot = "".join(f'<p>{html.escape(p[:700])}<span class=fn>{html.escape(p[700:900] or "See the references.")}</span></p>' for p in ps)
emit("w10-footnotes", wp(".fn{float:footnote;font-size:8pt}", foot), [p[:700] + " " + (p[700:900] or "See the references.") for p in ps], layout="footnotes", ordered=False)
ps = take(EN4, 16000, 70)
emit("w11-watermark", wp(".wm{position:fixed;top:40%;left:10%;font-size:70pt;color:#ddd;transform:rotate(-30deg)}", '<div class=wm>DRAFT</div>' + P(ps)), ps, layout="watermark", ordered=False)
ps = take(EN, 12000, 90)
emit("w12-colored-bg", wp("body{background:#1b2a49;color:#f2f2f2}", P(ps)), ps, layout="1col")
ps = take(EN3, 9000, 70)
emit("w13-mixed-fonts", wp("", "".join(f'<p style="font:{rnd.choice(["10pt serif","13pt sans-serif","9pt monospace","italic 11pt serif","bold 11pt sans-serif"])}">{html.escape(p)}</p>' for p in ps)), ps, layout="mixed-fonts")
ps = take(RFC, 30000)
emit("w14-monospace-code", wp("pre{font:8.5pt/1.2 'DejaVu Sans Mono',monospace;white-space:pre-wrap}", "<pre>" + html.escape("\n\n".join(ps)) + "</pre>"), ps, layout="monospace")
ps = take(EN4, 12000, 100)
emit("w15-ligature-font", wp("body{font-family:'Linux Libertine O',serif;font-size:12pt;font-variant-ligatures:common-ligatures}", P(ps)), ps, layout="ligatures")
ps = take(EN, 14000, 120)
emit("w16-slides-landscape", wp("@page{size:A4 landscape;margin:1.5cm} section{page-break-after:always;font-size:22pt} h2{font-size:34pt}", "".join(f"<section><h2>{html.escape(p.split('.')[0][:60])}</h2><p>{html.escape(p[:260])}</p></section>" for p in ps[:14])), [x for p in ps[:14] for x in (p.split('.')[0][:60], p[:260])], layout="slides")
ps = take(EN3, 30000, 90)
emit("w17-one-sentence-per-page", wp("p{page-break-after:always;font-size:16pt}", P([p.split(". ")[0] for p in ps[:60]])), [p.split(". ")[0] for p in ps[:60]], layout="many-pages")

# ---------- tables (row-major truth) ----------
words = " ".join(EN).split(); W = lambda: rnd.choice(words)
def table(rows, cols, spans=False):
    head = [f"Col{j} {W()}" for j in range(cols)]
    body = [[f"{W()} {rnd.randint(1, 9999)}" for _ in range(cols)] for _ in range(rows)]
    h = "<table border=1 cellpadding=3 style='border-collapse:collapse;width:100%;font-size:9pt'><tr>" + "".join(f"<th>{c}</th>" for c in head) + "</tr>"
    h += "".join("<tr>" + "".join(f"<td>{c}</td>" for c in r) + "</tr>" for r in body) + "</table>"
    return h, [" ".join(head)] + [" ".join(r) for r in body]
h, t = table(120, 4); emit("t01-table-4col", wp("", h), t, layout="table")
h, t = table(60, 10); emit("t02-table-10col-landscape", wp("@page{size:A4 landscape;margin:1cm}", h), t, layout="table")
rows = [[W()] + [f"{W()} {rnd.randint(1, 99)}" for _ in range(3)] for _ in range(60)]
h = "<table border=1 cellpadding=4 style='border-collapse:collapse;width:100%'><tr><th rowspan=2>Group</th><th colspan=3>Values</th></tr><tr><th>A</th><th>B</th><th>C</th></tr>" + "".join("<tr>" + "".join(f"<td>{c}</td>" for c in r) + "</tr>" for r in rows) + "</table>"
emit("t03-table-merged-cells", wp("", h), ["Group Values", "A B C"] + [" ".join(r) for r in rows], layout="table", ordered=False)
h, t = table(40, 3)
emit("t04-two-tables-side-by-side", wp(".s{column-count:2}", f"<div class=s>{h}</div>"), t, layout="table", ordered=False)
ps = take(EN, 8000, 150); h, t = table(25, 5)
emit("t05-text-and-table-mixed", wp("", P(ps[:4]) + h + P(ps[4:])), ps[:4] + t + ps[4:], layout="table-mixed")
items = [(f"{W()} {W()}", rnd.randint(1, 9), rnd.randint(5, 900)) for _ in range(30)]
inv = "<h2>INVOICE 2026-0042</h2><p>Bill to: Acme Corp, 1 Main Street</p><table border=1 cellpadding=3 style='border-collapse:collapse;width:100%'><tr><th>Item</th><th>Qty</th><th>Price</th><th>Total</th></tr>" + "".join(f"<tr><td>{i}</td><td>{q}</td><td>{p}.00</td><td>{q*p}.00</td></tr>" for i, q, p in items) + f"</table><p>Total due: {sum(q*p for _, q, p in items)}.00</p>"
emit("t06-invoice", wp("", inv), ["INVOICE 2026-0042", "Bill to: Acme Corp, 1 Main Street", "Item Qty Price Total"] + [f"{i} {q} {p}.00 {q*p}.00" for i, q, p in items] + [f"Total due: {sum(q*p for _, q, p in items)}.00"], layout="invoice")
form = "".join(f'<div style="position:absolute;left:2cm;top:{2+i*1.4}cm;font-size:10pt">{lab}: ______________________</div>' for i, lab in enumerate(["First name", "Last name", "Street", "City", "Postal code", "Phone", "E-mail", "Date of birth", "Signature", "Date"]))
emit("t07-form-absolute", wp("", form), [f"{l}: ______________________" for l in ["First name", "Last name", "Street", "City", "Postal code", "Phone", "E-mail", "Date of birth", "Signature", "Date"]], layout="form", ordered=False)

# ---------- other scripts ----------
for code, lang, rtl, size in (("ar", "arabic", True, 12), ("he", "hebrew", True, 12), ("ja", "japanese", False, 11), ("zh", "chinese", False, 11), ("ru", "cyrillic", False, 11), ("el", "greek", False, 11), ("de", "german", False, 11)):
    ps = take(paragraphs(f"wiki-{code}", 1), 14000)
    css = f"body{{font-family:'Noto Sans','Noto Sans Arabic','Noto Sans Hebrew','Noto Sans CJK JP','Noto Sans CJK SC',sans-serif;font-size:{size}pt}}" + ("body{direction:rtl}" if rtl else "")
    emit(f"s-{code}-{lang}", wp(css, P(ps)), ps, layout="script", lang=code, rtl=rtl)
ps = take(paragraphs("wiki-ar", 1), 14000, 20)
emit("s-ar-2col", wp("body{direction:rtl;column-count:2;font-family:'Noto Sans Arabic'}", P(ps)), ps, layout="script", lang="ar", rtl=True)
ps = take(paragraphs("wiki-ja", 1), 12000, 20)
emit("s-ja-2col", wp("body{column-count:2;font-family:'Noto Sans CJK JP'}", P(ps)), ps, layout="script", lang="ja")

# ---------- pdfTeX ----------
def latex(name, pre, ps, body_fn, **tags):
    ps = [ascii_(p) for p in ps]
    def w(path):
        src = pre + "\\begin{document}\n" + body_fn(ps) + "\n\\end{document}\n"
        with tempfile.TemporaryDirectory() as d:
            (Path(d) / "a.tex").write_text(src, encoding="utf-8")
            subprocess.run(["pdflatex", "-interaction=nonstopmode", "-halt-on-error", "a.tex"], cwd=d, check=True, capture_output=True)
            Path(path).write_bytes((Path(d) / "a.pdf").read_bytes())
    emit(name, w, ps, producer="pdftex", **tags)
esc = lambda s: re.sub(r"([&%$#_{}])", r"\\\1", s).replace("~", "\\~{}").replace("^", "\\^{}").replace("\\", "\\textbackslash{}") if False else re.sub(r"([&%$#_{}])", r"\\\1", s)
join = lambda ps: "\n\n".join(esc(p) for p in ps)
ps = take(EN, 30000, 10)
latex("l01-latex-article-ot1-ligatures", "\\documentclass[11pt]{article}\n", ps, join, layout="latex-ligatures")
latex("l02-latex-twocolumn-t1", "\\documentclass[10pt,twocolumn]{article}\\usepackage[T1]{fontenc}\\usepackage{lmodern}\\usepackage{cmap}\\usepackage[margin=2cm]{geometry}\n", take(EN3, 30000, 20), join, layout="multicolumn")
latex("l03-latex-footnotes", "\\documentclass[11pt]{article}\\usepackage[T1]{fontenc}\\usepackage{lmodern}\n", take(EN4, 16000, 40),
      lambda ps: "\n\n".join(esc(p[:600]) + "\\footnote{" + esc(p[600:800] or "See the references.") + "}" for p in ps), layout="footnotes", ordered=False)
latex("l04-latex-sections-headers", "\\documentclass[11pt,twoside]{book}\\usepackage[T1]{fontenc}\\usepackage{lmodern}\\usepackage{fancyhdr}\\pagestyle{fancy}\n", take(EN, 24000, 25),
      lambda ps: "\n".join(f"\\chapter{{Part {i+1}}}\n" + esc(p) for i, p in enumerate(ps)), layout="header-footer", ordered=False)

# ---------- reportlab: rotated pages and an invisible OCR text layer ----------
from reportlab.lib.pagesizes import A4
from reportlab.lib.styles import getSampleStyleSheet
from reportlab.platypus import Paragraph
from reportlab.pdfgen import canvas
from pypdf import PdfReader, PdfWriter
def upright(path, paras):
    c = canvas.Canvas(str(path), pagesize=A4); ss = getSampleStyleSheet(); y = 800
    for p in paras:
        q = Paragraph(html.escape(p), ss["BodyText"]); _, h = q.wrap(480, 800)
        if y - h < 40: c.showPage(); y = 800
        q.drawOn(c, 50, y - h); y -= h + 12
    c.save()
def rotated(path):
    tmp = Path(path).with_suffix(".tmp.pdf"); upright(tmp, ps_rot); r, w = PdfReader(str(tmp)), PdfWriter()
    for i, pg in enumerate(r.pages): pg.rotate((90, 180, 270)[i % 3]); w.add_page(pg)
    w.write(str(path)); tmp.unlink()
ps_rot = [p[:500] for p in take(EN, 14000, 200)]
emit("r01-rotated-pages", rotated, ps_rot, producer="reportlab", layout="rotated")
def ocr_layer(path):  # invisible text (render mode 3) over a grey box: what a scanned page with OCR looks like
    c = canvas.Canvas(str(path), pagesize=A4)
    for pg in range(0, len(ps_ocr), 2):
        c.setFillGray(.9); c.rect(30, 30, 535, 780, fill=1, stroke=0); t = c.beginText(50, 790); t.setTextRenderMode(3); t.setFont("Helvetica", 10)
        for p in ps_ocr[pg:pg + 2]:
            for ln in re.findall(r".{1,95}(?:\s|$)", p): t.textLine(ln.strip())
            t.textLine("")
        c.drawText(t); c.showPage()
    c.save()
ps_ocr = [ascii_(p) for p in take(EN3, 9000, 140)]
emit("r02-invisible-ocr-text", ocr_layer, ps_ocr, producer="reportlab", layout="ocr-layer")

# ---------- Ghostscript re-distilled copies (a second producer of the same text) ----------
for src, nm in (("w04-2col", "g01-gs-2col"), ("l01-latex-article-ot1-ligatures", "g02-gs-latex"), ("t01-table-4col", "g03-gs-table")):
    def w(path, s=src): subprocess.run(["gs", "-q", "-dNOPAUSE", "-dBATCH", "-sDEVICE=pdfwrite", f"-sOutputFile={path}", str(OUT / f"{s}.pdf")], check=True)
    emit(nm, w, (OUT / f"{src}.txt").read_text().split("\n"), producer="ghostscript", layout=json.loads((OUT / f"{src}.json").read_text())["layout"])
