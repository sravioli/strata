| Path | Shape | Library | Runs | Transactions/s | p50 | p99 | Spread tps / p99 |
|---|---|---|---|---|---|---|---|
| direct | read | sqlx | 5 | 36,008 | 1.76 ms | 2.32 ms | 10 % / 37 % |
| direct | read | tp | 5 | 44,836 | 1.41 ms | 1.83 ms | 7 % / 19 % |
| direct | read | tp-pipe | 5 | 97,876 | 0.64 ms | 0.88 ms | 11 % / 47 % |
| direct | status | sqlx | 5 | 19,620 | 3.22 ms | 4.20 ms | 5 % / 17 % |
| direct | status | tp | 5 | 23,017 | 2.75 ms | 3.69 ms | 9 % / 23 % |
| direct | status | tp-pipe | 5 | 31,645 | 2.01 ms | 2.62 ms | 5 % / 31 % |
| direct | search | sqlx | 5 | 403 | 158 ms | 175 ms | 1 % / 16 % |
| direct | search | tp | 5 | 400 | 159 ms | 176 ms | 4 % / 20 % |
| direct | search | tp-pipe | 5 | 406 | 156 ms | 175 ms | 1 % / 15 % |
| direct | insert | sqlx | 5 | 26 | 2428 ms | 2992 ms | 8 % / 11 % |
| direct | insert | tp | 5 | 24 | 2504 ms | 3194 ms | 12 % / 13 % |
| direct | insert | tp-pipe | 5 | 26 | 2438 ms | 3098 ms | 10 % / 11 % |
| latency1ms | read | sqlx | 5 | 2,483 | 25.73 ms | 27.46 ms | 0 % / 2 % |
| latency1ms | read | tp | 5 | 3,096 | 20.61 ms | 22.06 ms | 0 % / 2 % |
| latency1ms | read | tp-pipe | 5 | 11,728 | 5.44 ms | 6.02 ms | 1 % / 5 % |
| latency1ms | status | sqlx | 5 | 2,217 | 28.68 ms | 30.91 ms | 2 % / 13 % |
| latency1ms | status | tp | 5 | 2,761 | 23.12 ms | 25.02 ms | 1 % / 12 % |
| latency1ms | status | tp-pipe | 5 | 9,224 | 6.89 ms | 8.06 ms | 1 % / 13 % |
| latency1ms | search | sqlx | 5 | 366 | 173 ms | 198 ms | 2 % / 21 % |
| latency1ms | search | tp | 5 | 373 | 170 ms | 190 ms | 4 % / 30 % |
| latency1ms | search | tp-pipe | 5 | 402 | 158 ms | 179 ms | 2 % / 24 % |
| latency1ms | insert | sqlx | 5 | 27 | 2339 ms | 2995 ms | 12 % / 18 % |
| latency1ms | insert | tp | 5 | 26 | 2460 ms | 3187 ms | 4 % / 14 % |
| latency1ms | insert | tp-pipe | 5 | 26 | 2438 ms | 3073 ms | 11 % / 22 % |
| pgbouncer | read | sqlx | 5 | 19,180 | 3.32 ms | 4.00 ms | 1 % / 5 % |
| pgbouncer | read | tp | 5 | 23,587 | 2.70 ms | 3.21 ms | 2 % / 2 % |
| pgbouncer | read | tp-pipe | 5 | 41,223 | 1.54 ms | 1.86 ms | 1 % / 1 % |
| pgbouncer | status | sqlx | 5 | 14,665 | 4.33 ms | 5.38 ms | 2 % / 16 % |
| pgbouncer | status | tp | 5 | 17,383 | 3.65 ms | 4.51 ms | 2 % / 21 % |
| pgbouncer | status | tp-pipe | 5 | 24,526 | 2.59 ms | 3.10 ms | 1 % / 18 % |
| pgbouncer | search | sqlx | 5 | 394 | 162 ms | 178 ms | 2 % / 14 % |
| pgbouncer | search | tp | 5 | 397 | 160 ms | 177 ms | 2 % / 21 % |
| pgbouncer | search | tp-pipe | 5 | 406 | 156 ms | 175 ms | 2 % / 15 % |
| pgbouncer | insert | sqlx | 5 | 26 | 2444 ms | 2990 ms | 7 % / 9 % |
| pgbouncer | insert | tp | 5 | 26 | 2524 ms | 3018 ms | 7 % / 9 % |
| pgbouncer | insert | tp-pipe | 5 | 26 | 2412 ms | 3063 ms | 9 % / 12 % |

20 % rule: tokio-postgres (best of tp, tp-pipe) against sqlx, by median. A gain counts as real only when it is larger than the spread of both cells.

| Path | Shape | Best tp | tps gain | p99 gain | Real? |
|---|---|---|---|---|---|
| direct | read | tp-pipe | +172 % | +62 % | yes |
| direct | status | tp-pipe | +61 % | +38 % | yes |
| direct | search | tp-pipe | +1 % | +0 % | no |
| direct | insert | tp-pipe | +0 % | -4 % | no |
| latency1ms | read | tp-pipe | +372 % | +78 % | yes |
| latency1ms | status | tp-pipe | +316 % | +74 % | yes |
| latency1ms | search | tp-pipe | +10 % | +10 % | no |
| latency1ms | insert | tp-pipe | -2 % | -3 % | no |
| pgbouncer | read | tp-pipe | +115 % | +54 % | yes |
| pgbouncer | status | tp-pipe | +67 % | +42 % | yes |
| pgbouncer | search | tp-pipe | +3 % | +2 % | no |
| pgbouncer | insert | tp | -0 % | -1 % | no |
