#!/bin/sh
# Host side: download the public PDFs and the source texts. A failed URL is skipped (and listed).
set -u
cd "$(dirname "$0")"; mkdir -p corpus/public corpus/src
UA="pdf-bench/1.0"
n=0
while read -r u; do
  n=$((n+1)); f=corpus/public/$(printf '%s' "$u" | sed 's#https\?://##; s#[^A-Za-z0-9._-]#_#g' | cut -c1-80).pdf
  [ -s "$f" ] && continue
  curl -fsSL -A "$UA" --max-time 120 --max-filesize 25000000 -o "$f.part" "$u" 2>/dev/null \
    && [ "$(head -c4 "$f.part")" = "%PDF" ] && mv "$f.part" "$f" || { rm -f "$f.part"; echo "skip: $u" >&2; }
done < public.txt
while IFS='|' read -r name u; do
  [ -s "corpus/src/$name.txt" ] || curl -fsSL -A "$UA" --max-time 120 -o "corpus/src/$name.txt" "$u" || echo "skip text: $u" >&2
done < texts.txt
for l in en:"Portable Document Format" en2:"Industrial Revolution" en3:"History of the Internet" en4:"Roman Empire" en5:"Photosynthesis" en6:"Plate tectonics" de:Berlin ja:東京 zh:北京 ar:القاهرة he:ירושלים ru:Москва el:Αθήνα; do
  lang=${l%%:*}; t=${l#*:}; host=${lang%%[0-9]*}; [ -s "corpus/src/wiki-$lang.txt" ] && continue
  curl -fsSL -A "$UA" -G "https://$host.wikipedia.org/w/api.php" --data-urlencode "titles=$t" \
    -d action=query -d prop=extracts -d explaintext=1 -d redirects=1 -d format=json -o "corpus/src/wiki-$lang.json" \
    && python3 -c "import json,sys;d=json.load(open('corpus/src/wiki-$lang.json'));print(next(iter(d['query']['pages'].values()))['extract'])" > "corpus/src/wiki-$lang.txt" \
    || echo "skip wiki: $lang" >&2
done
echo "public pdfs: $(ls corpus/public/*.pdf 2>/dev/null | wc -l); texts: $(ls corpus/src/*.txt | wc -l)"
