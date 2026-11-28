#!/bin/sh
# Fetch (host), build the image, generate the corpus, run the bench under limits, write out/report.md.
set -eu
cd "$(dirname "$0")"; mkdir -p corpus out
./fetch.sh
podman build -q -t strata-pdf-bench -f Containerfile .
R="podman run --rm --network none --memory 4g --cpus 2 --security-opt no-new-privileges --cap-drop all"
[ -n "$(ls corpus/generated/*.pdf 2>/dev/null)" ] || $R -v "$PWD":/bench:ro -v "$PWD/corpus":/corpus -w /bench strata-pdf-bench python3 gen_corpus.py /corpus/src /corpus/generated
$R -v "$PWD":/bench:ro -v "$PWD/corpus":/corpus -w /bench strata-pdf-bench python3 mutate.py /corpus/public /corpus/mutated
# One CPU for the measured run: the same budget for every tool.
$R --cpus 1 -v "$PWD":/bench:ro -v "$PWD/corpus":/corpus:ro -v "$PWD/../../crates/strata-tests/fixtures":/fixtures:ro \
  -v "$PWD/out":/out -w /bench -e REPS="${REPS:-3}" strata-pdf-bench sh -c '
  pdftotext -v 2>&1 | head -1; mutool -v 2>&1 | head -1; pip list 2>/dev/null | grep -i pdfium
  python3 run.py generated=/corpus/generated public=/corpus/public fixtures=/fixtures mutated=/corpus/mutated > /out/log.jsonl && python3 report.py'
