"""Damaged copies of small real PDFs: what extractors do with a broken file in the wild."""
import random, sys
from pathlib import Path
src, out = Path(sys.argv[1]), Path(sys.argv[2]); out.mkdir(parents=True, exist_ok=True)
rnd = random.Random(5)
bases = sorted(p for p in src.glob("*.pdf") if p.stat().st_size < 700_000)[:5]
for p in bases:
    b = p.read_bytes(); n = p.stem[:28]
    def corrupt(b, rate):
        a = bytearray(b)
        for _ in range(max(1, int(len(a) * rate))): a[rnd.randrange(len(a))] = rnd.randrange(256)
        return bytes(a)
    for kind, data in (("trunc50", b[: len(b) // 2]), ("trunc90", b[: len(b) * 9 // 10]), ("zero-tail", b[:-4096] + bytes(4096)),
                       ("flip-0.1pct", corrupt(b, 0.001)), ("flip-2pct", corrupt(b, 0.02)), ("junk-prefix", bytes(rnd.randrange(256) for _ in range(1500)) + b)):
        (out / f"m-{n}-{kind}.pdf").write_bytes(data)
print(len(list(out.glob("*.pdf"))), "mutated files")
