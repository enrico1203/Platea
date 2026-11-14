#!/usr/bin/env python3
"""Libreria sintetica per i benchmark dello scanner: 40.000 film e 60.000 episodi vuoti.
Uso: docker run --rm -v "$PWD:/w" python:3-alpine python /w/scripts/bench_library.py /w/bench-media
"""
import os, random, sys
root = sys.argv[1]
random.seed(1)
words = "the of a dark night star war love city last king man world dead house blue red return secret lost day black time road river storm iron".split()
n = 0
# 40.000 film
for i in range(40000):
    t = " ".join(random.choice(words).capitalize() for _ in range(random.randint(1, 4))) + f" {i}"
    y = random.randint(1950, 2026)
    d = os.path.join(root, "Film", f"{t} ({y})")
    os.makedirs(d, exist_ok=True)
    open(os.path.join(d, f"{t} ({y}) - 1080p.mkv"), "w").close()
    n += 1
# 1.200 serie x 5 stagioni x 10 episodi = 60.000
for s in range(1200):
    t = " ".join(random.choice(words).capitalize() for _ in range(random.randint(1, 3))) + f" Show {s}"
    for se in range(1, 6):
        d = os.path.join(root, "Serie", t, f"Season {se:02d}")
        os.makedirs(d, exist_ok=True)
        for e in range(1, 11):
            open(os.path.join(d, f"{t}.S{se:02d}E{e:02d}.1080p.WEB-DL.x264.mkv"), "w").close()
            n += 1
print(n)
