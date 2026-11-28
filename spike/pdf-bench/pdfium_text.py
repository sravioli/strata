"""PDFium text extraction as a command, like pdftotext: PDF path in, text out, each page ends with \f.
Stderr: `t=<seconds>` (extraction without interpreter start) and `ctl=<n>` (stray control characters
that PDFium returned inside page text; they are replaced with a space so that \f stays the page break)."""
import re, sys, time
import pypdfium2 as pdfium

try:
    t0 = time.perf_counter()
    pdf = pdfium.PdfDocument(sys.argv[1])
    out, stray = [], 0
    for page in pdf:
        text, n = re.subn(r"[\x00-\x08\x0b\x0c\x0e-\x1f]", " ", page.get_textpage().get_text_range())
        stray += n
        out.append(text + "\f")
    sys.stdout.write("".join(out))
    sys.stderr.write(f"t={time.perf_counter() - t0:.6f}\nctl={stray}\n")
except pdfium.PdfiumError as e:
    sys.stderr.write(f"pdfium error: {e}\n")
    sys.exit(1)
