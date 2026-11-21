"""Median and spread (min-max) of each cell over the runs, and the 20 % rule of #100.
Usage: python3 summary.py results.jsonl"""
import json
import statistics
import sys
from collections import defaultdict

cells = defaultdict(list)
for line in open(sys.argv[1]):
    if line.startswith("{"):
        r = json.loads(line)
        cells[(r["path"], r["shape"], r["lib"])].append(r)


def med(rs, k):
    return statistics.median(r[k] for r in rs)


def spread(rs, k):
    v = [r[k] for r in rs]
    m = statistics.median(v)
    return 100 * (max(v) - min(v)) / m


def fmt_us(us):
    return f"{us / 1000:.2f} ms" if us < 100_000 else f"{us / 1000:.0f} ms"


print("| Path | Shape | Library | Runs | Transactions/s | p50 | p99 | Spread tps / p99 |")
print("|---|---|---|---|---|---|---|---|")
for (path, shape, lib), rs in sorted(cells.items(), key=lambda c: (c[0][0], ["read", "status", "search", "insert"].index(c[0][1]), c[0][2])):
    print(f"| {path} | {shape} | {lib} | {len(rs)} | {med(rs, 'tps'):,.0f} | {fmt_us(med(rs, 'p50_us'))} "
          f"| {fmt_us(med(rs, 'p99_us'))} | {spread(rs, 'tps'):.0f} % / {spread(rs, 'p99_us'):.0f} % |")

print()
print("20 % rule: tokio-postgres (best of tp, tp-pipe) against sqlx, by median. "
      "A gain counts as real only when it is larger than the spread of both cells.")
print()
print("| Path | Shape | Best tp | tps gain | p99 gain | Real? |")
print("|---|---|---|---|---|---|")
for path, shape in sorted({(p, s) for p, s, _ in cells}, key=lambda c: (c[0], ["read", "status", "search", "insert"].index(c[1]))):
    s = cells[(path, shape, "sqlx")]
    best = None
    for lib in ("tp", "tp-pipe"):
        t = cells[(path, shape, lib)]
        tps_gain = 100 * (med(t, "tps") / med(s, "tps") - 1)
        p99_gain = 100 * (1 - med(t, "p99_us") / med(s, "p99_us"))
        if best is None or max(tps_gain, p99_gain) > max(best[1], best[2]):
            best = (lib, tps_gain, p99_gain, t)
    lib, tps_gain, p99_gain, t = best
    noise_tps = max(spread(s, "tps"), spread(t, "tps"))
    noise_p99 = max(spread(s, "p99_us"), spread(t, "p99_us"))
    real = (tps_gain >= 20 and tps_gain > noise_tps) or (p99_gain >= 20 and p99_gain > noise_p99)
    print(f"| {path} | {shape} | {lib} | {tps_gain:+.0f} % | {p99_gain:+.0f} % | {'yes' if real else 'no'} |")
